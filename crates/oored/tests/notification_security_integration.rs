#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;

async fn owner_session(pool: &sqlx::SqlitePool, user_id: &str) -> String {
    let token = oored::token::generate_session_token();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(oored::token::hash_token(&token))
    .bind(user_id)
    .bind(common::now_unix())
    .bind(common::now_unix() + 3600)
    .execute(pool)
    .await
    .unwrap();
    token
}

#[tokio::test]
async fn smtp_authority_change_requires_a_fresh_password() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    let owner_id = common::seed_test_user(&pool).await;
    let token = owner_session(&pool, &owner_id).await;

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/settings/notification-channels")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "name": "Build email",
                        "channel_type": "email",
                        "enabled": false,
                        "events": [],
                        "smtp_config": {
                            "host": "smtp.original.invalid",
                            "port": 587,
                            "username": "builds",
                            "password": "original-password",
                            "tls_mode": "start_tls",
                            "from_address": "builds@example.com",
                            "recipients": ["qa@example.com"]
                        }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(create.status(), StatusCode::OK);
    let channel_id = common::body_json(create.into_body()).await["channel"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let rebind = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/settings/notification-channels/{channel_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "smtp_config": {"host": "smtp.replacement.invalid"}
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rebind.status(), StatusCode::BAD_REQUEST);

    let safe_update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/settings/notification-channels/{channel_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "smtp_config": {"recipients": ["release@example.com"]}
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(safe_update.status(), StatusCode::OK);

    let replacement = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/v1/settings/notification-channels/{channel_id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::from(
                    serde_json::to_vec(&serde_json::json!({
                        "smtp_config": {
                            "host": "smtp.replacement.invalid",
                            "password": "replacement-password"
                        }
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(replacement.status(), StatusCode::OK);
}
