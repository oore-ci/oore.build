#![cfg(feature = "test-support")]

mod common;

use std::sync::{Mutex, OnceLock};

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use common::{body_json, connect_pool, create_test_app, now_unix};
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

#[tokio::test]
async fn test_external_access_preflight_reports_failures() {
    let _env_guard = env_lock().lock().expect("env lock");
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
    let _env_guard = env_lock().lock().expect("env lock");
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
    let _env_guard = env_lock().lock().expect("env lock");
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
