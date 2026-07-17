#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use common::{body_json, connect_pool, create_test_app, now_unix, seed_test_user};
use tower::ServiceExt;

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

/// Seed the setup_state row to `ready` with OIDC config and an encrypted secret.
async fn seed_ready_with_oidc(pool: &sqlx::SqlitePool, has_secret: bool) {
    let now = now_unix();
    let encrypted_secret = if has_secret {
        Some(
            oored::crypto::encrypt("test-client-secret", &common::TEST_ENCRYPTION_KEY)
                .expect("failed to encrypt test secret"),
        )
    } else {
        None
    };

    sqlx::query(
        r#"UPDATE setup_state SET
            setup_state = 'ready',
            oidc_issuer_url = 'https://accounts.google.com',
            oidc_client_id = 'test-client-id',
            oidc_has_client_secret = ?1,
            oidc_authorization_endpoint = 'https://accounts.google.com/o/oauth2/v2/auth',
            oidc_token_endpoint = 'https://oauth2.googleapis.com/token',
            oidc_userinfo_endpoint = 'https://openidconnect.googleapis.com/v1/userinfo',
            oidc_jwks_uri = 'https://www.googleapis.com/oauth2/v3/certs',
            oidc_configured_at = ?2,
            oidc_encrypted_client_secret = ?3,
            oidc_secret_stored_at = ?4,
            owner_email = 'owner@example.com',
            owner_oidc_subject = 'owner-sub',
            owner_created_at = ?2,
            updated_at = ?2
        WHERE id = 1"#,
    )
    .bind(has_secret as i32)
    .bind(now)
    .bind(&encrypted_secret)
    .bind(if has_secret { Some(now) } else { None })
    .execute(pool)
    .await
    .expect("failed to seed ready state with OIDC config");
}

/// Seed the setup_state row to `ready` without OIDC config.
async fn seed_ready_without_oidc(pool: &sqlx::SqlitePool) {
    let now = now_unix();
    sqlx::query(
        r#"UPDATE setup_state SET
            setup_state = 'ready',
            owner_email = 'owner@example.com',
            owner_oidc_subject = 'owner-sub',
            owner_created_at = ?1,
            updated_at = ?1
        WHERE id = 1"#,
    )
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed ready state without OIDC");
}

// ── GET /v1/settings/external-access/oidc ───────────────────────

#[tokio::test]
async fn test_get_oidc_returns_config_with_secret() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    seed_ready_with_oidc(&pool, true).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(
        json["issuer_url"].as_str().unwrap(),
        "https://accounts.google.com"
    );
    assert_eq!(json["client_id"].as_str().unwrap(), "test-client-id");
    assert_eq!(json["has_client_secret"].as_bool(), Some(true));
    assert_eq!(
        json["authorization_endpoint"].as_str().unwrap(),
        "https://accounts.google.com/o/oauth2/v2/auth"
    );
    assert_eq!(
        json["token_endpoint"].as_str().unwrap(),
        "https://oauth2.googleapis.com/token"
    );
    assert_eq!(
        json["jwks_uri"].as_str().unwrap(),
        "https://www.googleapis.com/oauth2/v3/certs"
    );
    assert!(json["configured_at"].as_i64().is_some());
}

#[tokio::test]
async fn test_get_oidc_returns_config_without_secret() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    seed_ready_with_oidc(&pool, false).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["has_client_secret"].as_bool(), Some(false));
}

#[tokio::test]
async fn test_get_oidc_returns_404_when_not_configured() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    seed_ready_without_oidc(&pool).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["code"].as_str().unwrap(), "oidc_not_configured");
}

#[tokio::test]
async fn test_get_oidc_returns_conflict_before_ready() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    // Default setup_state is bootstrap_pending — do NOT advance to ready
    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("GET")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["code"].as_str().unwrap(), "invalid_state");
}

#[tokio::test]
async fn test_developer_cannot_read_oidc_config() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    seed_ready_with_oidc(&pool, true).await;

    let now = now_unix();
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
         VALUES (?1, 'dev@example.com', 'dev-sub', 'Dev', 'developer', 'active', ?2, ?2)",
    )
    .bind(&user_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let token = create_session_token(&pool, &user_id).await;

    let req = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("GET")
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ── PUT /v1/settings/external-access/oidc (configure) ───────────

#[tokio::test]
async fn test_configure_oidc_rejects_before_ready() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let body = serde_json::json!({
        "issuer_url": "https://accounts.google.com",
        "client_id": "my-client-id",
    });

    let req = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["code"].as_str().unwrap(), "invalid_state");
}

#[tokio::test]
async fn test_developer_cannot_configure_oidc() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    seed_ready_with_oidc(&pool, false).await;

    let now = now_unix();
    let user_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
         VALUES (?1, 'dev@example.com', 'dev-sub', 'Dev', 'developer', 'active', ?2, ?2)",
    )
    .bind(&user_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let token = create_session_token(&pool, &user_id).await;

    let body = serde_json::json!({
        "issuer_url": "https://accounts.google.com",
        "client_id": "my-client-id",
    });

    let req = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn oidc_secret_is_preserved_only_for_the_same_issuer_and_client() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;
    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    seed_ready_with_oidc(&pool, true).await;

    let unchanged = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(
            serde_json::json!({
                "issuer_url": "https://accounts.google.com",
                "client_id": "test-client-id"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(unchanged).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response.into_body()).await;
    assert_eq!(body["has_client_secret"], true);

    let changed_without_secret = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(
            serde_json::json!({
                "issuer_url": "https://replacement.example.com",
                "client_id": "replacement-client"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.clone().oneshot(changed_without_secret).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response.into_body()).await;
    assert_eq!(body["code"], "oidc_secret_reentry_required");

    let stored_client_id: String =
        sqlx::query_scalar("SELECT oidc_client_id FROM setup_state WHERE id = 1")
            .fetch_one(&pool)
            .await
            .expect("stored client id");
    assert_eq!(stored_client_id, "test-client-id");

    let changed_with_secret = Request::builder()
        .uri("/v1/settings/external-access/oidc")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(
            serde_json::json!({
                "issuer_url": "https://replacement.example.com",
                "client_id": "replacement-client",
                "client_secret": "replacement-secret"
            })
            .to_string(),
        ))
        .unwrap();
    let response = app.oneshot(changed_with_secret).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let encrypted: String =
        sqlx::query_scalar("SELECT oidc_encrypted_client_secret FROM setup_state WHERE id = 1")
            .fetch_one(&pool)
            .await
            .expect("replacement secret");
    assert_eq!(
        oored::crypto::decrypt(&encrypted, &common::TEST_ENCRYPTION_KEY).unwrap(),
        "replacement-secret"
    );
}
