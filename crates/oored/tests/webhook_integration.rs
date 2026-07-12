// Webhook integration tests — exercises GitHub/GitLab webhook handlers end-to-end.
// Run with: cargo test -p oored --features test-support
#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use hyper::Request;
use tower::ServiceExt;

// ── GitHub webhook tests ──────────────────────────────────────

#[tokio::test]
async fn test_github_webhook_happy_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gh-test-secret-happy";
    let integration_id = common::seed_github_integration(&pool, &user_id, secret).await;
    let (project_id, _pipeline_id) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "test-org/test-repo").await;

    let payload = common::github_push_payload("test-org/test-repo", "main", "abc123");
    let body_bytes = serde_json::to_vec(&payload).unwrap();
    let signature = common::github_hmac_signature(&body_bytes, secret);

    let req = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &signature)
        .header("x-github-delivery", "delivery-happy-1")
        .header("x-github-event", "push")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["ok"], true);
    assert!(json.get("duplicate").is_none() || json["duplicate"] != true);

    // Wait for async build creation
    let builds = common::wait_for_builds(&pool, &project_id, 1, 2000).await;
    assert_eq!(builds.len(), 1, "expected 1 build, got {}", builds.len());
    assert_eq!(builds[0]["build_number"], 1);
    assert_eq!(builds[0]["status"], "queued");
    assert_eq!(builds[0]["trigger_type"], "webhook");
    assert_eq!(builds[0]["trigger_event"], "push");
    assert_eq!(builds[0]["branch"], "main");
    assert_eq!(builds[0]["commit_sha"], "abc123");
}

#[tokio::test]
async fn test_github_webhook_idempotency() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gh-test-secret-idemp";
    let integration_id = common::seed_github_integration(&pool, &user_id, secret).await;
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "test-org/idemp-repo").await;

    let payload = common::github_push_payload("test-org/idemp-repo", "main", "def456");
    let body_bytes = serde_json::to_vec(&payload).unwrap();
    let signature = common::github_hmac_signature(&body_bytes, secret);

    // First delivery
    let req1 = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &signature)
        .header("x-github-delivery", "delivery-idemp-1")
        .header("x-github-event", "push")
        .body(Body::from(body_bytes.clone()))
        .unwrap();

    let resp1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), 200);

    // Wait for build from first delivery
    let builds = common::wait_for_builds(&pool, &project_id, 1, 2000).await;
    assert_eq!(builds.len(), 1);

    // Second delivery (duplicate)
    let req2 = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &signature)
        .header("x-github-delivery", "delivery-idemp-1")
        .header("x-github-event", "push")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), 200);

    let json2 = common::body_json(resp2.into_body()).await;
    assert_eq!(json2["duplicate"], true);

    // Give async processing a moment, then confirm no extra build
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let builds_after = common::wait_for_builds(&pool, &project_id, 1, 500).await;
    assert_eq!(
        builds_after.len(),
        1,
        "duplicate should not create extra build"
    );
}

#[tokio::test]
async fn test_github_webhook_invalid_signature() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let _integration_id = common::seed_github_integration(&pool, &user_id, "real-secret").await;

    let payload = common::github_push_payload("test-org/repo", "main", "aaa");
    let body_bytes = serde_json::to_vec(&payload).unwrap();
    // Sign with wrong secret
    let bad_signature = common::github_hmac_signature(&body_bytes, "wrong-secret");

    let req = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &bad_signature)
        .header("x-github-delivery", "delivery-bad-sig")
        .header("x-github-event", "push")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 401);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["code"], "invalid_signature");
}

#[tokio::test]
async fn test_github_webhook_secret_rotation_refreshes_cache_without_restart() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let old_secret = "gh-rotate-old-secret";
    let new_secret = "gh-rotate-new-secret";
    let integration_id = common::seed_github_integration(&pool, &user_id, old_secret).await;
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "org/rotate-repo").await;

    // First webhook uses old secret and warms the in-process cache.
    let payload1 = common::github_push_payload("org/rotate-repo", "main", "sha-old");
    let body1 = serde_json::to_vec(&payload1).unwrap();
    let sig1 = common::github_hmac_signature(&body1, old_secret);
    let req1 = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &sig1)
        .header("x-github-delivery", "delivery-rotate-1")
        .header("x-github-event", "push")
        .body(Body::from(body1))
        .unwrap();
    let resp1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), 200);
    let builds1 = common::wait_for_builds(&pool, &project_id, 1, 2000).await;
    assert_eq!(builds1.len(), 1);

    // Rotate the stored webhook secret in DB.
    let encrypted_new = oored::crypto::encrypt(new_secret, &common::TEST_ENCRYPTION_KEY)
        .expect("failed to encrypt rotated secret");
    sqlx::query(
        "UPDATE integration_credentials \
         SET encrypted_value = ?1, updated_at = ?2 \
         WHERE integration_id = ?3 AND credential_type = 'webhook_secret'",
    )
    .bind(&encrypted_new)
    .bind(common::now_unix())
    .bind(&integration_id)
    .execute(&pool)
    .await
    .expect("failed to rotate webhook secret");

    // Second webhook uses new secret. It should still succeed without restart
    // by forcing a cache refresh after the initial cached-signature miss.
    let payload2 = common::github_push_payload("org/rotate-repo", "main", "sha-new");
    let body2 = serde_json::to_vec(&payload2).unwrap();
    let sig2 = common::github_hmac_signature(&body2, new_secret);
    let req2 = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &sig2)
        .header("x-github-delivery", "delivery-rotate-2")
        .header("x-github-event", "push")
        .body(Body::from(body2))
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), 200);

    let builds2 = common::wait_for_builds(&pool, &project_id, 2, 2000).await;
    assert_eq!(
        builds2.len(),
        2,
        "rotated secret should still trigger a build"
    );
    assert_eq!(builds2[1]["commit_sha"], "sha-new");
}

#[tokio::test]
async fn test_github_webhook_missing_signature() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;

    let payload = common::github_push_payload("org/repo", "main", "aaa");
    let body_bytes = serde_json::to_vec(&payload).unwrap();

    let req = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-github-delivery", "delivery-no-sig")
        .header("x-github-event", "push")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 401);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["code"], "missing_signature");
}

#[tokio::test]
async fn test_github_webhook_payload_too_large() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;

    // 1 MB + 1 byte
    let big_body = vec![b'x'; 1_048_577];
    let signature = common::github_hmac_signature(&big_body, "any");

    let req = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &signature)
        .header("x-github-delivery", "delivery-big")
        .header("x-github-event", "push")
        .body(Body::from(big_body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 413);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["code"], "payload_too_large");
}

#[tokio::test]
async fn test_github_webhook_invalid_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gh-test-secret-badjson";
    let _integration_id = common::seed_github_integration(&pool, &user_id, secret).await;

    let bad_json = b"not valid json{{{";
    let signature = common::github_hmac_signature(bad_json, secret);

    let req = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &signature)
        .header("x-github-delivery", "delivery-badjson")
        .header("x-github-event", "push")
        .body(Body::from(bad_json.to_vec()))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 400);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["code"], "invalid_payload");
}

// ── GitLab webhook tests ──────────────────────────────────────

#[tokio::test]
async fn test_gitlab_webhook_happy_path() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gl-test-secret-happy";
    let integration_id = common::seed_gitlab_integration(&pool, &user_id, secret).await;
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "test-group/test-project")
            .await;

    let payload = common::gitlab_push_payload("test-group/test-project", "main", "gl-sha-1");
    let body_bytes = serde_json::to_vec(&payload).unwrap();

    let req = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", secret)
        .header("x-gitlab-event-uuid", "gl-delivery-happy-1")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["ok"], true);

    let builds = common::wait_for_builds(&pool, &project_id, 1, 2000).await;
    assert_eq!(builds.len(), 1, "expected 1 build, got {}", builds.len());
    assert_eq!(builds[0]["build_number"], 1);
    assert_eq!(builds[0]["status"], "queued");
    assert_eq!(builds[0]["trigger_type"], "webhook");
    assert_eq!(builds[0]["trigger_event"], "Push Hook");
    assert_eq!(builds[0]["branch"], "main");
    assert_eq!(builds[0]["commit_sha"], "gl-sha-1");
}

#[tokio::test]
async fn test_gitlab_webhook_idempotency() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gl-test-idemp";
    let integration_id = common::seed_gitlab_integration(&pool, &user_id, secret).await;
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "group/idemp-proj").await;

    let payload = common::gitlab_push_payload("group/idemp-proj", "dev", "gl-sha-idemp");
    let body_bytes = serde_json::to_vec(&payload).unwrap();

    // First delivery
    let req1 = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", secret)
        .header("x-gitlab-event-uuid", "gl-delivery-idemp-1")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(body_bytes.clone()))
        .unwrap();

    let resp1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), 200);

    let builds = common::wait_for_builds(&pool, &project_id, 1, 2000).await;
    assert_eq!(builds.len(), 1);

    // Duplicate delivery
    let req2 = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", secret)
        .header("x-gitlab-event-uuid", "gl-delivery-idemp-1")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), 200);

    let json2 = common::body_json(resp2.into_body()).await;
    assert_eq!(json2["duplicate"], true);

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let builds_after = common::wait_for_builds(&pool, &project_id, 1, 500).await;
    assert_eq!(
        builds_after.len(),
        1,
        "duplicate should not create extra build"
    );
}

#[tokio::test]
async fn test_gitlab_webhook_idempotency_without_event_uuid() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gl-test-fallback-idemp";
    let integration_id = common::seed_gitlab_integration(&pool, &user_id, secret).await;
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "group/fallback-proj").await;
    let body = serde_json::to_vec(&common::gitlab_push_payload(
        "group/fallback-proj",
        "main",
        "gl-sha-fallback",
    ))
    .unwrap();

    for expected_duplicate in [false, true] {
        let request = Request::post("/v1/webhooks/gitlab")
            .header("content-type", "application/json")
            .header("x-gitlab-token", secret)
            .header("x-gitlab-event", "Push Hook")
            .body(Body::from(body.clone()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200);
        let json = common::body_json(response.into_body()).await;
        assert_eq!(
            json["duplicate"].as_bool().unwrap_or(false),
            expected_duplicate
        );
        if !expected_duplicate {
            common::wait_for_builds(&pool, &project_id, 1, 2000).await;
        }
    }

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let builds = common::wait_for_builds(&pool, &project_id, 1, 500).await;
    assert_eq!(builds.len(), 1);
}

#[tokio::test]
async fn test_gitlab_webhook_secret_rotation_refreshes_cache_without_restart() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let old_secret = "gl-rotate-old-secret";
    let new_secret = "gl-rotate-new-secret";
    let integration_id = common::seed_gitlab_integration(&pool, &user_id, old_secret).await;
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "group/rotate-proj").await;

    // First webhook uses old secret and warms the in-process cache.
    let payload1 = common::gitlab_push_payload("group/rotate-proj", "main", "gl-sha-old");
    let body1 = serde_json::to_vec(&payload1).unwrap();
    let req1 = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", old_secret)
        .header("x-gitlab-event-uuid", "gl-delivery-rotate-1")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(body1))
        .unwrap();
    let resp1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), 200);
    let builds1 = common::wait_for_builds(&pool, &project_id, 1, 2000).await;
    assert_eq!(builds1.len(), 1);

    // Rotate stored webhook secret in DB.
    let encrypted_new = oored::crypto::encrypt(new_secret, &common::TEST_ENCRYPTION_KEY)
        .expect("failed to encrypt rotated secret");
    sqlx::query(
        "UPDATE integration_credentials \
         SET encrypted_value = ?1, updated_at = ?2 \
         WHERE integration_id = ?3 AND credential_type = 'webhook_secret'",
    )
    .bind(&encrypted_new)
    .bind(common::now_unix())
    .bind(&integration_id)
    .execute(&pool)
    .await
    .expect("failed to rotate webhook secret");

    // Second webhook uses new secret. It should still succeed without restart
    // by forcing a cache refresh after the initial cached-token miss.
    let payload2 = common::gitlab_push_payload("group/rotate-proj", "main", "gl-sha-new");
    let body2 = serde_json::to_vec(&payload2).unwrap();
    let req2 = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", new_secret)
        .header("x-gitlab-event-uuid", "gl-delivery-rotate-2")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(body2))
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), 200);

    let builds2 = common::wait_for_builds(&pool, &project_id, 2, 2000).await;
    assert_eq!(
        builds2.len(),
        2,
        "rotated token should still trigger a build"
    );
    assert_eq!(builds2[1]["commit_sha"], "gl-sha-new");
}

#[tokio::test]
async fn test_gitlab_webhook_wrong_token() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let _integration_id = common::seed_gitlab_integration(&pool, &user_id, "correct-token").await;

    let payload = common::gitlab_push_payload("group/repo", "main", "sha1");
    let body_bytes = serde_json::to_vec(&payload).unwrap();

    let req = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", "wrong-token")
        .header("x-gitlab-event-uuid", "gl-wrong-token")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 401);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["code"], "invalid_token");
}

#[tokio::test]
async fn test_gitlab_webhook_missing_token() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;

    let payload = common::gitlab_push_payload("group/repo", "main", "sha1");
    let body_bytes = serde_json::to_vec(&payload).unwrap();

    let req = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-event-uuid", "gl-no-token")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 401);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["code"], "missing_token");
}

#[tokio::test]
async fn test_gitlab_webhook_stale_event() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gl-stale-secret";
    let _integration_id = common::seed_gitlab_integration(&pool, &user_id, secret).await;

    // Create a payload with a timestamp 6 minutes in the past
    let stale_time = chrono::Utc::now() - chrono::Duration::seconds(360);
    let payload = serde_json::json!({
        "ref": "refs/heads/main",
        "checkout_sha": "stale-sha",
        "project": {
            "path_with_namespace": "group/stale-repo",
            "web_url": "https://gitlab.com/group/stale-repo"
        },
        "user_username": "test-user",
        "created_at": stale_time.to_rfc3339()
    });
    let body_bytes = serde_json::to_vec(&payload).unwrap();

    let req = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", secret)
        .header("x-gitlab-event-uuid", "gl-stale-1")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 400);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["code"], "stale_event");
}

#[tokio::test]
async fn test_gitlab_webhook_payload_too_large() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;

    let big_body = vec![b'x'; 1_048_577];

    let req = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", "any")
        .header("x-gitlab-event-uuid", "gl-big")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(big_body))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 413);

    let json = common::body_json(resp.into_body()).await;
    assert_eq!(json["code"], "payload_too_large");
}

// ── Cross-cutting tests ───────────────────────────────────────

#[tokio::test]
async fn test_webhook_triggers_only_linked_project() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gh-linked-secret";

    // Two integrations, each linked to a different repo
    let integration_a = common::seed_github_integration(&pool, &user_id, secret).await;
    let integration_b = common::seed_github_integration(&pool, &user_id, "other-secret").await;

    let (project_a, _) =
        common::seed_project_chain(&pool, &integration_a, &user_id, "org/repo-a").await;
    let (project_b, _) =
        common::seed_project_chain(&pool, &integration_b, &user_id, "org/repo-b").await;

    // Webhook for repo-a
    let payload = common::github_push_payload("org/repo-a", "main", "sha-linked");
    let body_bytes = serde_json::to_vec(&payload).unwrap();
    let signature = common::github_hmac_signature(&body_bytes, secret);

    let req = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &signature)
        .header("x-github-delivery", "delivery-linked-1")
        .header("x-github-event", "push")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    // Wait for build in project A
    let builds_a = common::wait_for_builds(&pool, &project_a, 1, 2000).await;
    assert_eq!(builds_a.len(), 1, "project A should have 1 build");

    // Project B should have 0 builds
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let builds_b = common::wait_for_builds(&pool, &project_b, 0, 500).await;
    assert_eq!(builds_b.len(), 0, "project B should have 0 builds");
}

#[tokio::test]
async fn test_webhook_unknown_repo_no_builds() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gh-unknown-repo-secret";
    let integration_id = common::seed_github_integration(&pool, &user_id, secret).await;
    // Seed a chain for a DIFFERENT repo than what the webhook will deliver
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "org/known-repo").await;

    // Webhook for unknown repo
    let payload = common::github_push_payload("org/unknown-repo", "main", "sha-unknown");
    let body_bytes = serde_json::to_vec(&payload).unwrap();
    let signature = common::github_hmac_signature(&body_bytes, secret);

    let req = Request::post("/v1/webhooks/github")
        .header("content-type", "application/json")
        .header("x-hub-signature-256", &signature)
        .header("x-github-delivery", "delivery-unknown-1")
        .header("x-github-event", "push")
        .body(Body::from(body_bytes))
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200); // Webhook ACKs even if no project matches

    // No builds should be created
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let builds = common::wait_for_builds(&pool, &project_id, 0, 500).await;
    assert_eq!(builds.len(), 0, "unknown repo should produce no builds");
}

// ── GitLab OAuth callback tests ───────────────────────────────

#[tokio::test]
async fn test_gitlab_callback_rejects_bad_redirect_origin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let integration_id = common::seed_gitlab_oauth_integration(
        &pool,
        &user_id,
        "webhook-secret",
        "test-client-id",
        "test-client-secret",
    )
    .await;

    // Seal a state token with an evil redirect URL
    let state = common::seal_gitlab_oauth_state(&integration_id, "https://evil.com/steal");

    let req = Request::get(format!(
        "/v1/integrations/gitlab/callback?code=test-code&state={}",
        state
    ))
    .body(Body::empty())
    .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // Should return 200 with an HTML error page (not a redirect)
    assert_eq!(resp.status(), 200);

    let body = common::body_string(resp.into_body()).await;
    assert!(
        body.contains("does not match the configured frontend origin"),
        "expected redirect-origin rejection message, got: {body}"
    );
}

#[tokio::test]
async fn test_gitlab_oauth_failure_keeps_integration_inactive() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let integration_id = common::seed_gitlab_oauth_integration(
        &pool,
        &user_id,
        "webhook-secret",
        "test-client-id",
        "test-client-secret",
    )
    .await;

    // Use a redirect URL that matches the default OORE_CORS_ORIGIN (http://localhost:3000)
    let redirect_url = format!(
        "http://localhost:3000/settings/integrations/{}",
        integration_id
    );
    let state = common::seal_gitlab_oauth_state(&integration_id, &redirect_url);

    let req = Request::get(format!(
        "/v1/integrations/gitlab/callback?code=fake-code&state={}",
        state
    ))
    .body(Body::empty())
    .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    // exchange_gitlab_code will fail because there's no real GitLab server to POST to.
    // The handler should return an HTML error page.
    assert_eq!(resp.status(), 200);

    let body = common::body_string(resp.into_body()).await;
    assert!(
        body.contains("Authorization failed") || body.contains("Failed to complete"),
        "expected exchange failure error page, got: {body}"
    );

    // Verify the integration is still inactive — user can retry authorization
    let row: (String,) = sqlx::query_as("SELECT status FROM integrations WHERE id = ?1")
        .bind(&integration_id)
        .fetch_one(&pool)
        .await
        .expect("failed to query integration status");
    assert_eq!(
        row.0, "inactive",
        "integration should remain inactive after failed OAuth exchange"
    );
}
