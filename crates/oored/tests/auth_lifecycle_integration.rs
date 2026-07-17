#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

async fn seed_user(pool: &sqlx::SqlitePool, email: &str, role: &str) -> String {
    let user_id = uuid::Uuid::new_v4().to_string();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?2, ?4, 'active', ?5, ?5)",
    )
    .bind(&user_id)
    .bind(email)
    .bind(format!("subject::{email}"))
    .bind(role)
    .bind(now)
    .execute(pool)
    .await
    .expect("seed user");
    user_id
}

async fn create_session(pool: &sqlx::SqlitePool, user_id: &str) -> String {
    let token = oored::token::generate_session_token();
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(oored::token::hash_token(&token))
    .bind(user_id)
    .bind(now)
    .bind(now + 86_400)
    .execute(pool)
    .await
    .expect("seed session");
    token
}

#[tokio::test]
async fn api_token_role_tracks_creator_demotion_without_widening_on_promotion() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let _app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    let user_id = seed_user(&pool, "admin@example.com", "admin").await;

    let (_, admin_token, _, _) =
        oored::api_tokens::create_api_token(&pool, &user_id, "admin", "admin", None)
            .await
            .expect("create admin token");
    assert_eq!(
        oored::api_tokens::validate_api_token(&pool, &admin_token)
            .await
            .expect("validate admin token")
            .expect("admin token valid")
            .role,
        "admin"
    );

    sqlx::query("UPDATE users SET role = 'developer' WHERE id = ?1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("demote creator");
    assert_eq!(
        oored::api_tokens::validate_api_token(&pool, &admin_token)
            .await
            .expect("validate demoted token")
            .expect("demoted token valid")
            .role,
        "developer"
    );

    let (_, developer_token, _, _) =
        oored::api_tokens::create_api_token(&pool, &user_id, "developer", "developer", None)
            .await
            .expect("create developer token");
    sqlx::query("UPDATE users SET role = 'admin' WHERE id = ?1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .expect("promote creator");
    assert_eq!(
        oored::api_tokens::validate_api_token(&pool, &developer_token)
            .await
            .expect("validate narrow token")
            .expect("narrow token valid")
            .role,
        "developer"
    );
}

#[tokio::test]
async fn disabling_user_permanently_revokes_existing_api_tokens() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    let owner_id = seed_user(&pool, "owner@example.com", "owner").await;
    let developer_id = seed_user(&pool, "developer@example.com", "developer").await;
    let owner_session = create_session(&pool, &owner_id).await;
    let _developer_session = create_session(&pool, &developer_id).await;
    let (_, old_token, _, _) =
        oored::api_tokens::create_api_token(&pool, &developer_id, "pre-disable", "developer", None)
            .await
            .expect("create old token");

    let disable = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/users/{developer_id}"))
                .header("authorization", format!("Bearer {owner_session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("disable user");
    assert_eq!(disable.status(), StatusCode::OK);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM api_tokens WHERE created_by = ?1 AND revoked_at IS NOT NULL",
        )
        .bind(&developer_id)
        .fetch_one(&pool)
        .await
        .expect("count revoked tokens"),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE user_id = ?1")
            .bind(&developer_id)
            .fetch_one(&pool)
            .await
            .expect("count sessions"),
        0
    );

    let enable = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/users/{developer_id}/enable"))
                .header("authorization", format!("Bearer {owner_session}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("enable user");
    assert_eq!(enable.status(), StatusCode::OK);
    assert!(
        oored::api_tokens::validate_api_token(&pool, &old_token)
            .await
            .expect("validate old token")
            .is_none()
    );

    let (_, new_token, _, _) =
        oored::api_tokens::create_api_token(&pool, &developer_id, "post-enable", "developer", None)
            .await
            .expect("create replacement token");
    assert!(
        oored::api_tokens::validate_api_token(&pool, &new_token)
            .await
            .expect("validate replacement token")
            .is_some()
    );
}
