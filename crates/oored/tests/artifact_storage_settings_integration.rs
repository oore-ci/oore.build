#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use axum::http::{self, Request, StatusCode};
use common::{
    body_json, connect_pool, create_test_app, create_test_app_with_public_url, now_unix,
    seed_github_integration, seed_project_chain, seed_test_user,
};
use http_body_util::BodyExt;
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

async fn register_runner(app: &axum::Router, session_token: &str, name: &str) -> (String, String) {
    let body = serde_json::json!({
        "name": name,
        "capabilities": { "os": "macos", "arch": "arm64" }
    });

    let req = Request::builder()
        .uri("/v1/runners/register")
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {session_token}"),
        )
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let runner_id = json["runner"]["id"].as_str().unwrap().to_string();
    let runner_token = json["token"].as_str().unwrap().to_string();

    (runner_id, runner_token)
}

async fn seed_running_build(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    pipeline_id: &str,
    runner_id: &str,
) -> String {
    let build_id = uuid::Uuid::new_v4().to_string();
    let now = now_unix();

    sqlx::query(
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, runner_id, \
         trigger_type, config_snapshot, queued_at, started_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, \
                 (SELECT COALESCE(MAX(build_number), 0) + 1 FROM builds WHERE project_id = ?2), \
                 'running', ?4, 'manual', '{}', ?5, ?5, ?5, ?5)",
    )
    .bind(&build_id)
    .bind(project_id)
    .bind(pipeline_id)
    .bind(runner_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed running build");

    build_id
}

async fn complete_artifact(
    app: &axum::Router,
    runner_id: &str,
    runner_token: &str,
    build_id: &str,
    artifact_id: &str,
) {
    let req = Request::builder()
        .uri(format!(
            "/v1/runners/{runner_id}/jobs/{build_id}/artifacts/{artifact_id}/complete"
        ))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from("{}"))
        .unwrap();
    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_owner_can_configure_local_storage_and_download_artifact() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "test/local-artifacts").await;

    let (runner_id, runner_token) = register_runner(&app, &owner_session, "local-runner").await;
    let build_id = seed_running_build(&pool, &project_id, &pipeline_id, &runner_id).await;

    let local_dir = tmp.path().join("artifacts");
    let update_body = serde_json::json!({
        "provider": "local",
        "local_base_dir": local_dir.to_string_lossy(),
    });

    let req = Request::builder()
        .uri("/v1/settings/artifact-storage")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(serde_json::to_string(&update_body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(json["settings"]["provider"].as_str().unwrap(), "local");

    let artifact_body = serde_json::json!({
        "name": "app-release.apk",
        "artifact_type": "apk",
        "file_size": 4,
        "checksum": "abcd",
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&artifact_body).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let create_json = body_json(resp.into_body()).await;
    let upload_url = create_json["upload_url"].as_str().unwrap();
    assert!(upload_url.contains("/v1/artifacts/local-upload/"));

    let upload_path = url::Url::parse(upload_url).unwrap().path().to_string();
    let req = Request::builder()
        .uri(upload_path)
        .method("PUT")
        .body(Body::from(vec![1u8, 2, 3, 4]))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let artifact_id = create_json["artifact"]["id"].as_str().unwrap();
    complete_artifact(&app, &runner_id, &runner_token, &build_id, artifact_id).await;
    let req = Request::builder()
        .uri(format!("/v1/artifacts/{artifact_id}/download-link"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let link_json = body_json(resp.into_body()).await;
    let download_url = link_json["download_url"].as_str().unwrap();
    assert!(download_url.contains("/v1/artifacts/download/"));

    let download_path = url::Url::parse(download_url).unwrap().path().to_string();
    let req = Request::builder()
        .uri(download_path)
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), &[1u8, 2, 3, 4]);
}

#[tokio::test]
async fn test_warpgate_ios_install_keeps_ticket_through_local_download() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app_with_public_url(&db_path, "https://ci.example.com").await;
    let pool = connect_pool(&db_path).await;
    let now = now_unix();

    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, runtime_mode, remote_auth_mode, created_at, updated_at)
         VALUES (1, 'file', 'remote', 'trusted_proxy', ?1, ?1)
         ON CONFLICT(id) DO UPDATE SET runtime_mode = 'remote', remote_auth_mode = 'trusted_proxy', updated_at = ?1",
    )
    .bind(now)
    .execute(&pool)
    .await
    .expect("set Warpgate mode");
    let encrypted_ticket = oored::crypto::encrypt("ios ticket/+?", &common::TEST_ENCRYPTION_KEY)
        .expect("encrypt Warpgate ticket");
    sqlx::query(
        "INSERT INTO trusted_proxy_settings (id, user_email_header, trusted_proxy_cidrs_json, encrypted_warpgate_ticket, created_at, updated_at)
         VALUES (1, 'x-warpgate-username', '[]', ?1, ?2, ?2)",
    )
    .bind(encrypted_ticket)
    .bind(now)
    .execute(&pool)
    .await
    .expect("configure Warpgate ticket");

    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;
    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;
    let (project_id, pipeline_id) =
        seed_project_chain(&pool, &integration_id, &owner_id, "test/warpgate-ios").await;
    let (runner_id, runner_token) = register_runner(&app, &owner_session, "ios-runner").await;
    let build_id = seed_running_build(&pool, &project_id, &pipeline_id, &runner_id).await;

    let local_dir = tmp.path().join("artifacts");
    let request = Request::builder()
        .uri("/v1/settings/artifact-storage")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "provider": "local",
                "local_base_dir": local_dir.to_string_lossy(),
            }))
            .unwrap(),
        ))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let artifact_body = serde_json::json!({
        "name": "Kite.ipa",
        "artifact_type": "ipa",
        "file_size": 4,
        "checksum": "abcd",
        "metadata": {
            "ios_app": {
                "bundle_identifier": "com.example.kite",
                "display_name": "Kite QA",
                "version": "1.0.0",
                "build_number": "1"
            },
            "ios_signing": {
                "bundle_ids": ["com.example.kite"],
                "effective_export_method": "release-testing"
            }
        }
    });
    let request = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&artifact_body).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let created = body_json(response.into_body()).await;
    let upload_url = url::Url::parse(created["upload_url"].as_str().unwrap()).unwrap();
    let request = Request::builder()
        .uri(upload_url.path())
        .method("PUT")
        .body(Body::from(vec![1u8, 2, 3, 4]))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let artifact_id = created["artifact"]["id"].as_str().unwrap();
    complete_artifact(&app, &runner_id, &runner_token, &build_id, artifact_id).await;
    let request = Request::builder()
        .uri(format!("/v1/artifacts/{artifact_id}/install-link"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let install = body_json(response.into_body()).await;

    let manifest_url = url::Url::parse(install["manifest_url"].as_str().unwrap()).unwrap();
    assert_eq!(
        manifest_url
            .query_pairs()
            .find(|(key, _)| key == "warpgate-ticket")
            .map(|(_, value)| value.into_owned())
            .as_deref(),
        Some("ios ticket/+?")
    );
    let manifest_uri = match manifest_url.query() {
        Some(query) => format!("{}?{query}", manifest_url.path()),
        None => manifest_url.path().to_string(),
    };
    let request = Request::builder()
        .uri(manifest_uri)
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let manifest = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(manifest.contains("warpgate-ticket=ios+ticket%2F%2B%3F"));

    let scoped_url = url::Url::parse(install["download_url"].as_str().unwrap()).unwrap();
    let scoped_uri = format!(
        "{}?{}",
        scoped_url.path(),
        scoped_url.query().expect("Warpgate query")
    );
    let request = Request::builder()
        .uri(scoped_uri)
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    let local_url = url::Url::parse(
        response
            .headers()
            .get(http::header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        local_url
            .query_pairs()
            .find(|(key, _)| key == "warpgate-ticket")
            .map(|(_, value)| value.into_owned())
            .as_deref(),
        Some("ios ticket/+?")
    );

    let local_uri = format!(
        "{}?{}",
        local_url.path(),
        local_url.query().expect("Warpgate query")
    );
    let request = Request::builder()
        .uri(local_uri)
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.as_ref(), &[1u8, 2, 3, 4]);
}

#[tokio::test]
async fn test_local_storage_large_artifact_upload_download() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_test_user(&pool).await;
    let owner_session = create_session_token(&pool, &owner_id).await;

    let integration_id = seed_github_integration(&pool, &owner_id, "secret").await;
    let (project_id, pipeline_id) = seed_project_chain(
        &pool,
        &integration_id,
        &owner_id,
        "test/local-artifacts-large",
    )
    .await;

    let (runner_id, runner_token) = register_runner(&app, &owner_session, "local-runner").await;
    let build_id = seed_running_build(&pool, &project_id, &pipeline_id, &runner_id).await;

    let local_dir = tmp.path().join("artifacts");
    let update_body = serde_json::json!({
        "provider": "local",
        "local_base_dir": local_dir.to_string_lossy(),
    });

    let req = Request::builder()
        .uri("/v1/settings/artifact-storage")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::from(serde_json::to_string(&update_body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let artifact_body = serde_json::json!({
        "name": "app-large.apk",
        "artifact_type": "apk",
        "file_size": 3 * 1024 * 1024,
        "checksum": "large-checksum",
    });

    let req = Request::builder()
        .uri(format!("/v1/runners/{runner_id}/jobs/{build_id}/artifacts"))
        .method("POST")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {runner_token}"),
        )
        .body(Body::from(serde_json::to_string(&artifact_body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let create_json = body_json(resp.into_body()).await;
    let upload_url = create_json["upload_url"].as_str().unwrap();
    let upload_path = url::Url::parse(upload_url).unwrap().path().to_string();

    let bytes = vec![7u8; 3 * 1024 * 1024];
    let req = Request::builder()
        .uri(upload_path)
        .method("PUT")
        .body(Body::from(bytes.clone()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let artifact_id = create_json["artifact"]["id"].as_str().unwrap();
    complete_artifact(&app, &runner_id, &runner_token, &build_id, artifact_id).await;
    let req = Request::builder()
        .uri(format!("/v1/artifacts/{artifact_id}/download-link"))
        .method("POST")
        .header(
            http::header::AUTHORIZATION,
            format!("Bearer {owner_session}"),
        )
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let link_json = body_json(resp.into_body()).await;
    let download_path = url::Url::parse(link_json["download_url"].as_str().unwrap())
        .unwrap()
        .path()
        .to_string();

    let req = Request::builder()
        .uri(download_path)
        .method("GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let downloaded = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(downloaded.len(), bytes.len());
    assert_eq!(downloaded.as_ref(), bytes.as_slice());
}

#[tokio::test]
async fn test_developer_cannot_modify_artifact_storage_settings() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

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
        .uri("/v1/settings/artifact-storage")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "provider": "disabled"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_owner_can_update_instance_preferences_key_storage_mode() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_test_user(&pool).await;
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
                "key_storage_mode": "file"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp.into_body()).await;
    assert_eq!(
        json["preferences"]["key_storage_mode"].as_str().unwrap(),
        "file"
    );
    assert_eq!(
        json["preferences"]["restart_required"].as_bool(),
        Some(false)
    );

    let req = Request::builder()
        .uri("/v1/settings/preferences")
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
        json["preferences"]["key_storage_mode"].as_str().unwrap(),
        "file"
    );
}

#[tokio::test]
async fn test_owner_cannot_set_keychain_mode_in_this_release() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

    let owner_id = seed_test_user(&pool).await;
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
                "key_storage_mode": "keychain"
            }))
            .unwrap(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let json = body_json(resp.into_body()).await;
    assert_eq!(
        json["code"].as_str().unwrap(),
        "unsupported_key_storage_mode"
    );
}

#[tokio::test]
async fn test_developer_cannot_modify_instance_preferences() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = create_test_app(&db_path).await;
    let pool = connect_pool(&db_path).await;

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
        .uri("/v1/settings/preferences")
        .method("PUT")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::from(
            serde_json::to_string(&serde_json::json!({
                "key_storage_mode": "file"
            }))
            .unwrap(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}
