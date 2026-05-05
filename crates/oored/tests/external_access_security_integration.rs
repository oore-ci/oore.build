#![cfg(feature = "test-support")]

mod common;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{self, Request, StatusCode};
use common::{body_json, connect_pool, create_test_app, now_unix};
use sqlx::Row;
use tokio::sync::Mutex;
use tower::ServiceExt;

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvVarGuard {
    key: &'static str,
    previous: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: Test helper scopes env mutations and restores values in Drop.
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }

    fn unset(key: &'static str) -> Self {
        let previous = std::env::var(key).ok();
        // SAFETY: Test helper scopes env mutations and restores values in Drop.
        unsafe {
            std::env::remove_var(key);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => {
                // SAFETY: Restores process env var to its original value.
                unsafe {
                    std::env::set_var(self.key, value);
                }
            }
            None => {
                // SAFETY: Removes process env var that was introduced by this guard.
                unsafe {
                    std::env::remove_var(self.key);
                }
            }
        }
    }
}

fn init_test_git_repo(root: &std::path::Path) -> PathBuf {
    let repo_path = root.join("repo");
    std::fs::create_dir_all(&repo_path).expect("create repo dir");

    let output = Command::new("git")
        .args(["-C", repo_path.to_str().unwrap(), "init"])
        .output()
        .expect("git init");
    assert!(
        output.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    repo_path
}

async fn create_session_token(pool: &sqlx::SqlitePool, user_id: &str) -> String {
    let token = oored::token::generate_session_token();
    let hashed = oored::token::hash_token(&token);
    let now = now_unix();
    let expires_at = now + 86400;

    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&hashed)
    .bind(user_id)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await
    .expect("failed to create test session");

    token
}

async fn seed_user_with_role(pool: &sqlx::SqlitePool, email: &str, role: &str) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    let now = now_unix();
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
    .expect("failed to seed test user");
    user_id
}

async fn mark_setup_ready_with_oidc(pool: &sqlx::SqlitePool) {
    let now = now_unix();
    sqlx::query(
        "UPDATE setup_state SET
            setup_state = 'ready',
            oidc_issuer_url = 'https://issuer.example.com',
            oidc_client_id = 'example-client-id',
            oidc_has_client_secret = 0,
            oidc_authorization_endpoint = 'https://issuer.example.com/auth',
            oidc_token_endpoint = 'https://issuer.example.com/token',
            oidc_userinfo_endpoint = 'https://issuer.example.com/userinfo',
            oidc_jwks_uri = 'https://issuer.example.com/jwks',
            oidc_configured_at = ?1,
            updated_at = ?1
         WHERE id = 1",
    )
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to update setup state");
}

async fn mark_setup_ready_without_oidc(pool: &sqlx::SqlitePool) {
    let now = now_unix();
    sqlx::query(
        "UPDATE setup_state SET
            setup_state = 'ready',
            oidc_issuer_url = NULL,
            oidc_client_id = NULL,
            oidc_has_client_secret = NULL,
            oidc_authorization_endpoint = NULL,
            oidc_token_endpoint = NULL,
            oidc_userinfo_endpoint = NULL,
            oidc_jwks_uri = NULL,
            oidc_configured_at = NULL,
            oidc_encrypted_client_secret = NULL,
            oidc_secret_stored_at = NULL,
            updated_at = ?1
         WHERE id = 1",
    )
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to update setup state");
}

async fn mark_setup_ready(pool: &sqlx::SqlitePool) {
    let now = now_unix();
    sqlx::query(
        "UPDATE setup_state SET
            setup_state = 'ready',
            updated_at = ?1
         WHERE id = 1",
    )
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to mark setup ready");
}

async fn set_runtime_and_remote_auth_mode(
    pool: &sqlx::SqlitePool,
    runtime_mode: &str,
    remote_auth_mode: &str,
) {
    let now = now_unix();
    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, runtime_mode, remote_auth_mode, created_at, updated_at)
         VALUES (1, 'file', ?1, ?2, ?3, ?3)
         ON CONFLICT(id) DO UPDATE SET
            key_storage_mode = excluded.key_storage_mode,
            runtime_mode = excluded.runtime_mode,
            remote_auth_mode = excluded.remote_auth_mode,
            updated_at = excluded.updated_at",
    )
    .bind(runtime_mode)
    .bind(remote_auth_mode)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to set runtime/auth mode");
}

async fn upsert_trusted_proxy_settings(
    pool: &sqlx::SqlitePool,
    user_email_header: &str,
    trusted_proxy_cidrs_json: &str,
) {
    upsert_trusted_proxy_settings_with_secret(
        pool,
        user_email_header,
        trusted_proxy_cidrs_json,
        None,
    )
    .await;
}

async fn upsert_trusted_proxy_settings_with_secret(
    pool: &sqlx::SqlitePool,
    user_email_header: &str,
    trusted_proxy_cidrs_json: &str,
    shared_secret: Option<&str>,
) {
    let now = now_unix();
    let encrypted_shared_secret = shared_secret.map(|secret| {
        oored::crypto::encrypt(secret, &common::TEST_ENCRYPTION_KEY)
            .expect("failed to encrypt trusted proxy shared secret")
    });
    sqlx::query(
        "INSERT INTO trusted_proxy_settings (id, user_email_header, trusted_proxy_cidrs_json, encrypted_shared_secret, updated_by, created_at, updated_at)
         VALUES (1, ?1, ?2, ?3, NULL, ?4, ?4)
         ON CONFLICT(id) DO UPDATE SET
            user_email_header = excluded.user_email_header,
            trusted_proxy_cidrs_json = excluded.trusted_proxy_cidrs_json,
            encrypted_shared_secret = excluded.encrypted_shared_secret,
            updated_at = excluded.updated_at",
    )
    .bind(user_email_header)
    .bind(trusted_proxy_cidrs_json)
    .bind(encrypted_shared_secret)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to upsert trusted proxy settings");
}

async fn seed_user_with_role_and_status(
    pool: &sqlx::SqlitePool,
    email: &str,
    role: &str,
    status: &str,
) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    let now = now_unix();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
    )
    .bind(&user_id)
    .bind(email)
    .bind(format!("{role}::{email}"))
    .bind(email)
    .bind(role)
    .bind(status)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed test user with status");
    user_id
}

async fn set_owner_created_with_setup_session(
    pool: &sqlx::SqlitePool,
    owner_email: &str,
    session_token: &str,
) {
    let now = now_unix();
    let session_expires_at = now + 1800;
    let session_hash = oored::token::hash_token(session_token);
    let owner_subject = format!("warpgate::{}", owner_email.to_lowercase());

    sqlx::query(
        "UPDATE setup_state SET
            setup_state = 'owner_created',
            owner_email = ?1,
            owner_oidc_subject = ?2,
            owner_created_at = ?3,
            session_hash = ?4,
            session_expires_at = ?5,
            updated_at = ?3
         WHERE id = 1",
    )
    .bind(owner_email)
    .bind(owner_subject)
    .bind(now)
    .bind(session_hash)
    .bind(session_expires_at)
    .execute(pool)
    .await
    .expect("failed to set owner_created setup state");
}

#[tokio::test]
async fn test_external_access_preflight_reports_failures() {
    let _env_guard = env_lock().lock().await;
    let _public_url = EnvVarGuard::unset("OORE_PUBLIC_URL");
    let _cors_origins = EnvVarGuard::unset("OORE_CORS_ORIGINS");
    let _cors_origin = EnvVarGuard::unset("OORE_CORS_ORIGIN");

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/preflight")
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.expect("preflight response");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["ready"], false);

    let checks = body["checks"].as_array().expect("checks array");
    let setup_check = checks
        .iter()
        .find(|check| check["id"] == "setup_ready")
        .expect("setup check");
    assert_eq!(setup_check["ok"], false);

    let url_check = checks
        .iter()
        .find(|check| check["id"] == "public_url_https")
        .expect("public url check");
    assert_eq!(url_check["ok"], false);
    assert_eq!(
        url_check["failure_code"].as_str(),
        Some("external_access_public_url_missing")
    );
}

#[tokio::test]
async fn test_external_access_preflight_all_checks_pass_with_valid_config() {
    let _env_guard = env_lock().lock().await;
    let _public_url = EnvVarGuard::set("OORE_PUBLIC_URL", "https://external.oore.test");
    let _cors = EnvVarGuard::set(
        "OORE_CORS_ORIGINS",
        "http://localhost:3000,http://127.0.0.1:3000,http://localhost:4173,http://127.0.0.1:4173,https://external.oore.test",
    );

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    mark_setup_ready_with_oidc(&pool).await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/preflight")
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.expect("preflight response");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["ready"], true);
    let checks = body["checks"].as_array().expect("checks array");
    assert!(
        checks
            .iter()
            .all(|check| check["ok"].as_bool() == Some(true))
    );
}

#[tokio::test]
async fn test_runtime_mode_switch_requires_owner() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let admin_id = seed_user_with_role(&pool, "admin@example.com", "admin").await;
    let admin_session = create_session_token(&pool, &admin_id).await;

    let req = Request::builder()
        .uri("/v1/settings/preferences")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {admin_session}"),
        )
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "key_storage_mode": "file",
                "runtime_mode": "remote"
            }))
            .expect("serialize request"),
        ))
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("preferences response");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "external_access_owner_required");
}

#[tokio::test]
async fn test_owner_cannot_enable_external_access_when_preflight_fails() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let req = Request::builder()
        .uri("/v1/settings/preferences")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "key_storage_mode": "file",
                "runtime_mode": "remote"
            }))
            .expect("serialize request"),
        ))
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("preferences response");
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "external_access_preflight_failed");
}

#[tokio::test]
async fn test_owner_can_enable_external_access_and_mode_change_revokes_sessions() {
    let _env_guard = env_lock().lock().await;
    let _public_url = EnvVarGuard::set("OORE_PUBLIC_URL", "https://external.oore.test");
    let _cors = EnvVarGuard::set(
        "OORE_CORS_ORIGINS",
        "http://localhost:3000,http://127.0.0.1:3000,http://localhost:4173,http://127.0.0.1:4173,https://external.oore.test",
    );

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    mark_setup_ready_with_oidc(&pool).await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let admin_id = seed_user_with_role(&pool, "admin@example.com", "admin").await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    let _admin_session = create_session_token(&pool, &admin_id).await;

    let req = Request::builder()
        .uri("/v1/settings/preferences")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "key_storage_mode": "file",
                "runtime_mode": "remote"
            }))
            .expect("serialize request"),
        ))
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("preferences response");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["preferences"]["runtime_mode"], "remote");

    let remaining_sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .expect("query sessions");
    assert_eq!(remaining_sessions, 0);
}

#[tokio::test]
async fn test_external_access_network_settings_update_requires_owner() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let admin_id = seed_user_with_role(&pool, "admin@example.com", "admin").await;
    let admin_session = create_session_token(&pool, &admin_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/network")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {admin_session}"),
        )
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41101))))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "public_url": "https://ci.oore.test",
                "allowed_origins": ["https://ci.oore.test"]
            }))
            .expect("serialize request"),
        ))
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("network settings response");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "external_access_owner_required");
}

#[tokio::test]
async fn test_external_access_network_settings_update_requires_loopback_in_local_mode() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/network")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .extension(ConnectInfo(SocketAddr::from(([10, 10, 0, 8], 41102))))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "public_url": "https://ci.oore.test",
                "allowed_origins": ["https://ci.oore.test"]
            }))
            .expect("serialize request"),
        ))
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("network settings response");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "external_access_loopback_required");
}

#[tokio::test]
async fn test_external_access_network_settings_update_rejected_when_forwarded_client_not_loopback()
{
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/network")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        // Peer connects over loopback, but forwarded headers indicate a non-loopback client.
        .header("x-forwarded-for", "127.0.0.1, 203.0.113.13")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41104))))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "public_url": "https://ci.oore.test",
                "allowed_origins": ["https://ci.oore.test"]
            }))
            .expect("serialize request"),
        ))
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("network settings response");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "external_access_loopback_required");
}

#[tokio::test]
async fn test_external_access_network_settings_update_and_readback() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let put_req = Request::builder()
        .uri("/v1/settings/external-access/network")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41103))))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "public_url": "https://ci.oore.test",
                "allowed_origins": ["https://ci.oore.test"]
            }))
            .expect("serialize request"),
        ))
        .unwrap();

    let put_resp = app
        .clone()
        .oneshot(put_req)
        .await
        .expect("network settings put response");
    assert_eq!(put_resp.status(), StatusCode::OK);
    let put_body = body_json(put_resp.into_body()).await;
    assert_eq!(
        put_body["settings"]["public_url"].as_str(),
        Some("https://ci.oore.test")
    );
    assert_eq!(put_body["settings"]["source"], "database");
    let origins = put_body["settings"]["allowed_origins"]
        .as_array()
        .expect("allowed origins array");
    assert!(
        origins
            .iter()
            .any(|origin| origin.as_str() == Some("https://ci.oore.test"))
    );
    assert!(
        origins
            .iter()
            .any(|origin| origin.as_str() == Some("http://localhost:3000"))
    );

    let get_req = Request::builder()
        .uri("/v1/settings/external-access/network")
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();
    let get_resp = app
        .clone()
        .oneshot(get_req)
        .await
        .expect("network settings get response");
    assert_eq!(get_resp.status(), StatusCode::OK);
    let get_body = body_json(get_resp.into_body()).await;
    assert_eq!(
        get_body["settings"]["public_url"].as_str(),
        Some("https://ci.oore.test")
    );
    assert_eq!(get_body["settings"]["source"], "database");
}

#[tokio::test]
async fn test_owner_can_configure_external_access_oidc_after_setup_ready() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    mark_setup_ready_without_oidc(&pool).await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "issuer_url": "https://accounts.google.com",
                "client_id": "client-id-123"
            }))
            .expect("serialize request"),
        ))
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("oidc configure response");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["discovered_issuer"], "https://accounts.google.com");
    assert_eq!(body["has_client_secret"], false);

    let oidc_client_id: Option<String> =
        sqlx::query_scalar("SELECT oidc_client_id FROM setup_state WHERE id = 1")
            .fetch_one(&pool)
            .await
            .expect("query oidc_client_id");
    assert_eq!(oidc_client_id.as_deref(), Some("client-id-123"));
}

#[tokio::test]
async fn test_non_owner_cannot_configure_external_access_oidc() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    mark_setup_ready_without_oidc(&pool).await;

    let admin_id = seed_user_with_role(&pool, "admin@example.com", "admin").await;
    let admin_session = create_session_token(&pool, &admin_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {admin_session}"),
        )
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "issuer_url": "https://accounts.google.com",
                "client_id": "client-id-123"
            }))
            .expect("serialize request"),
        ))
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("oidc configure response");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "external_access_owner_required");
}

#[tokio::test]
async fn trusted_proxy_login_rejects_untrusted_peer() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    mark_setup_ready(&pool).await;
    set_runtime_and_remote_auth_mode(&pool, "remote", "trusted_proxy").await;
    upsert_trusted_proxy_settings(&pool, "x-warpgate-username", "[]").await;

    let req = Request::builder()
        .uri("/v1/auth/trusted-proxy/login")
        .method("POST")
        .header("x-warpgate-username", "owner@example.com")
        .extension(ConnectInfo(SocketAddr::from(([10, 10, 0, 8], 43101))))
        .body(Body::empty())
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("trusted proxy login response");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "trusted_proxy_peer_not_allowed");
}

#[tokio::test]
async fn trusted_proxy_login_rejects_missing_identity_header() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    mark_setup_ready(&pool).await;
    set_runtime_and_remote_auth_mode(&pool, "remote", "trusted_proxy").await;
    upsert_trusted_proxy_settings(&pool, "x-warpgate-username", "[]").await;

    let req = Request::builder()
        .uri("/v1/auth/trusted-proxy/login")
        .method("POST")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 43102))))
        .body(Body::empty())
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("trusted proxy login response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "trusted_proxy_identity_missing");
}

#[tokio::test]
async fn trusted_proxy_login_activates_invited_user() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    mark_setup_ready(&pool).await;
    set_runtime_and_remote_auth_mode(&pool, "remote", "trusted_proxy").await;
    upsert_trusted_proxy_settings(&pool, "x-warpgate-username", "[]").await;

    let invited_user_id =
        seed_user_with_role_and_status(&pool, "invitee@example.com", "developer", "invited").await;

    let req = Request::builder()
        .uri("/v1/auth/trusted-proxy/login")
        .method("POST")
        .header("x-warpgate-username", "invitee@example.com")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 43103))))
        .body(Body::empty())
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("trusted proxy login response");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["user"]["email"], "invitee@example.com");
    assert_eq!(body["user"]["role"], "developer");
    assert!(body["session_token"].as_str().is_some());

    let row = sqlx::query("SELECT status, oidc_subject FROM users WHERE id = ?1")
        .bind(&invited_user_id)
        .fetch_one(&pool)
        .await
        .expect("query invited user");
    let status: String = row.get("status");
    let oidc_subject: String = row.get("oidc_subject");

    assert_eq!(status, "active");
    assert_eq!(oidc_subject, "warpgate::invitee@example.com");
}

#[tokio::test]
async fn trusted_proxy_login_requires_configured_shared_secret() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    mark_setup_ready(&pool).await;
    set_runtime_and_remote_auth_mode(&pool, "remote", "trusted_proxy").await;
    upsert_trusted_proxy_settings_with_secret(
        &pool,
        "x-warpgate-username",
        "[]",
        Some("proxy-secret"),
    )
    .await;
    seed_user_with_role(&pool, "owner@example.com", "owner").await;

    let missing_secret = Request::builder()
        .uri("/v1/auth/trusted-proxy/login")
        .method("POST")
        .header("x-warpgate-username", "owner@example.com")
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 43104))))
        .body(Body::empty())
        .unwrap();
    let resp = app
        .clone()
        .oneshot(missing_secret)
        .await
        .expect("missing secret response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "trusted_proxy_shared_secret_missing");

    let wrong_secret = Request::builder()
        .uri("/v1/auth/trusted-proxy/login")
        .method("POST")
        .header("x-warpgate-username", "owner@example.com")
        .header(
            oored::instance_settings::TRUSTED_PROXY_SHARED_SECRET_HEADER,
            "wrong-secret",
        )
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 43105))))
        .body(Body::empty())
        .unwrap();
    let resp = app
        .clone()
        .oneshot(wrong_secret)
        .await
        .expect("wrong secret response");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "trusted_proxy_shared_secret_invalid");

    let valid_secret = Request::builder()
        .uri("/v1/auth/trusted-proxy/login")
        .method("POST")
        .header("x-warpgate-username", "owner@example.com")
        .header(
            oored::instance_settings::TRUSTED_PROXY_SHARED_SECRET_HEADER,
            "proxy-secret",
        )
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 43106))))
        .body(Body::empty())
        .unwrap();
    let resp = app
        .clone()
        .oneshot(valid_secret)
        .await
        .expect("valid secret response");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["user"]["email"], "owner@example.com");
    assert!(body["session_token"].as_str().is_some());
}

#[tokio::test]
async fn external_access_preflight_uses_trusted_proxy_check_when_selected() {
    let _env_guard = env_lock().lock().await;
    let _public_url = EnvVarGuard::set("OORE_PUBLIC_URL", "https://external.oore.test");
    let _cors = EnvVarGuard::set(
        "OORE_CORS_ORIGINS",
        "http://localhost:3000,http://127.0.0.1:3000,http://localhost:4173,http://127.0.0.1:4173,https://external.oore.test",
    );

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    mark_setup_ready(&pool).await;
    set_runtime_and_remote_auth_mode(&pool, "remote", "trusted_proxy").await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/preflight")
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.expect("preflight response");
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    let checks = body["checks"].as_array().expect("checks array");

    let proxy_check = checks
        .iter()
        .find(|check| check["id"] == "trusted_proxy_configured")
        .expect("trusted proxy check");
    assert_eq!(proxy_check["ok"], false);
    assert_eq!(
        proxy_check["failure_code"].as_str(),
        Some("external_access_trusted_proxy_not_configured")
    );
    assert!(!checks.iter().any(|check| check["id"] == "oidc_configured"));
    assert!(
        !checks
            .iter()
            .any(|check| check["id"] == "redirect_policy_consistent")
    );
}

#[tokio::test]
async fn complete_setup_blocks_remote_trusted_proxy_if_not_configured() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    set_runtime_and_remote_auth_mode(&pool, "remote", "trusted_proxy").await;

    let setup_session_token = "setup-session-token";
    set_owner_created_with_setup_session(&pool, "owner@example.com", setup_session_token).await;

    let req = Request::builder()
        .uri("/v1/setup/complete")
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {setup_session_token}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("complete setup response");
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["code"], "remote_auth_not_configured");
}

#[tokio::test]
async fn test_create_project_local_repo_allowed_in_remote_mode() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_user_with_role(&pool, "owner@example.com", "owner").await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let now = now_unix();
    sqlx::query(
        "UPDATE instance_preferences SET runtime_mode = 'remote', updated_at = ?1 WHERE id = 1",
    )
    .bind(now)
    .execute(&pool)
    .await
    .expect("failed to set runtime mode");

    let repo_path = init_test_git_repo(tmp.path());
    let body = serde_json::json!({
        "name": "Local Repo Project",
        "local_repository_path": repo_path.to_string_lossy(),
    });

    let req = Request::builder()
        .uri("/v1/projects")
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(
            serde_json::to_string(&body).expect("serialize body"),
        ))
        .unwrap();

    let resp = app
        .clone()
        .oneshot(req)
        .await
        .expect("create project response");
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["project"]["name"], "Local Repo Project");
}
