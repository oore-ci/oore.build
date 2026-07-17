//! R086-001 local recovery capability integration coverage.
//! Run with: cargo test -p oored --features test-support --test local_recovery_integration
#![cfg(feature = "test-support")]

mod common;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use hyper::{Request, Response, StatusCode};
use oore_contract::{LocalRecoveryMintRequest, LocalRecoveryMintResponse};
use oored::local_recovery::{ManagementSocket, RecoveryCapabilityStore, management_socket_path};
use sqlx::SqlitePool;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tower::ServiceExt;

async fn seed_ready_remote(pool: &SqlitePool) -> String {
    common::set_runtime_mode(pool, "remote").await;
    sqlx::query("UPDATE setup_state SET setup_state = 'ready', updated_at = ?1 WHERE id = 1")
        .bind(common::now_unix())
        .execute(pool)
        .await
        .expect("mark setup ready");
    common::seed_test_user(pool).await
}

async fn spawn_management_socket(
    db_path: &Path,
    pool: SqlitePool,
    capabilities: RecoveryCapabilityStore,
) -> (PathBuf, tokio::task::JoinHandle<anyhow::Result<()>>) {
    let socket_path = management_socket_path(db_path);
    let socket = ManagementSocket::bind(socket_path.clone(), pool, capabilities)
        .await
        .expect("bind management socket");
    let task = tokio::spawn(socket.serve());
    (socket_path, task)
}

async fn mint(socket_path: &Path, email: Option<&str>, ttl_seconds: u64) -> String {
    let mut stream = UnixStream::connect(socket_path)
        .await
        .expect("connect management socket");
    let request = LocalRecoveryMintRequest {
        email: email.map(str::to_string),
        ttl_seconds,
    };
    let mut encoded = serde_json::to_vec(&request).expect("encode mint request");
    encoded.push(b'\n');
    stream
        .write_all(&encoded)
        .await
        .expect("write mint request");

    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .await
        .expect("read mint response");
    match serde_json::from_str::<LocalRecoveryMintResponse>(&response).expect("decode response") {
        LocalRecoveryMintResponse::Success { capability, .. } => capability,
        LocalRecoveryMintResponse::Error { error } => {
            panic!("mint failed with {}", error.code)
        }
    }
}

fn recovery_request(
    capability: Option<&str>,
    email: Option<&str>,
    headers: &[(&str, &str)],
) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/v1/auth/local/login")
        .header("content-type", "application/json")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 42001))));
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder
        .body(Body::from(
            serde_json::to_vec(&serde_json::json!({
                "email": email,
                "recovery_capability": capability,
            }))
            .expect("encode login request"),
        ))
        .expect("build login request")
}

async fn response_code(response: Response<Body>) -> String {
    common::body_json(response.into_body()).await["code"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn unix_socket_capability_succeeds_once_and_raw_secret_is_not_audited() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let capabilities = RecoveryCapabilityStore::default();
    let app = common::create_test_app_with_recovery(&db_path, capabilities.clone()).await;
    let pool = common::connect_pool(&db_path).await;
    seed_ready_remote(&pool).await;
    let (socket_path, socket_task) =
        spawn_management_socket(&db_path, pool.clone(), capabilities).await;
    let capability = mint(&socket_path, None, 60).await;

    let first = app
        .clone()
        .oneshot(recovery_request(Some(&capability), None, &[]))
        .await
        .expect("first recovery");
    assert_eq!(first.status(), StatusCode::OK);
    let second = app
        .oneshot(recovery_request(Some(&capability), None, &[]))
        .await
        .expect("replay recovery");
    assert_eq!(second.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        response_code(second).await,
        "local_recovery_capability_invalid"
    );

    let secret_audit_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE details LIKE ?1")
            .bind(format!("%{capability}%"))
            .fetch_one(&pool)
            .await
            .expect("check audit details");
    assert_eq!(secret_audit_rows, 0);
    let recovery_audit_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE action IN \
         ('local_recovery_capability_minted', 'local_recovery_login_succeeded', \
          'local_recovery_login_failed')",
    )
    .fetch_one(&pool)
    .await
    .expect("check recovery audits");
    assert_eq!(recovery_audit_rows, 3);

    socket_task.abort();
}

#[tokio::test]
async fn expired_malformed_unknown_and_wrong_account_capabilities_fail() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let capabilities = RecoveryCapabilityStore::default();
    let app = common::create_test_app_with_recovery(&db_path, capabilities.clone()).await;
    let pool = common::connect_pool(&db_path).await;
    seed_ready_remote(&pool).await;
    let (socket_path, socket_task) = spawn_management_socket(&db_path, pool, capabilities).await;

    let expired = mint(&socket_path, None, 1).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let wrong_account = mint(&socket_path, None, 60).await;
    let unknown = format!("oore_recovery_{}", "f".repeat(64));

    for (capability, email, expected_code) in [
        (expired.as_str(), None, "local_recovery_capability_invalid"),
        (
            "not-a-capability",
            None,
            "local_recovery_capability_invalid",
        ),
        (unknown.as_str(), None, "local_recovery_capability_invalid"),
        (
            wrong_account.as_str(),
            Some("other@example.com"),
            "local_recovery_account_mismatch",
        ),
    ] {
        let response = app
            .clone()
            .oneshot(recovery_request(Some(capability), email, &[]))
            .await
            .expect("rejected recovery");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response_code(response).await, expected_code);
    }

    socket_task.abort();
}

#[tokio::test]
async fn ready_remote_rejects_loopback_and_same_host_proxy_without_capability() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    seed_ready_remote(&pool).await;

    for headers in [
        Vec::new(),
        vec![("x-forwarded-for", "127.0.0.1")],
        vec![("forwarded", "for=127.0.0.1;proto=https")],
    ] {
        let response = app
            .clone()
            .oneshot(recovery_request(None, None, &headers))
            .await
            .expect("rejected recovery");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response_code(response).await,
            "local_recovery_capability_required"
        );
    }
}

#[tokio::test]
async fn concurrent_double_consumption_has_exactly_one_success() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let capabilities = RecoveryCapabilityStore::default();
    let app = common::create_test_app_with_recovery(&db_path, capabilities.clone()).await;
    let pool = common::connect_pool(&db_path).await;
    seed_ready_remote(&pool).await;
    let (socket_path, socket_task) = spawn_management_socket(&db_path, pool, capabilities).await;
    let capability = mint(&socket_path, None, 60).await;

    let first = app
        .clone()
        .oneshot(recovery_request(Some(&capability), None, &[]));
    let second = app.oneshot(recovery_request(Some(&capability), None, &[]));
    let (first, second) = tokio::join!(first, second);
    let statuses = [
        first.expect("first response").status(),
        second.expect("second response").status(),
    ];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::FORBIDDEN)
            .count(),
        1
    );

    socket_task.abort();
}
