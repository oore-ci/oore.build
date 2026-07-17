// Webhook integration tests — exercises GitHub/GitLab webhook handlers end-to-end.
// Run with: cargo test -p oored --features test-support
#![cfg(feature = "test-support")]

mod common;

use axum::body::Body;
use hyper::Request;
use tower::ServiceExt;

async fn repository_external_id(
    pool: &sqlx::SqlitePool,
    integration_id: &str,
    repository_full_name: &str,
) -> String {
    sqlx::query_scalar(
        "SELECT r.external_id \
         FROM integration_repositories r \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         WHERE inst.integration_id = ?1 AND r.full_name = ?2",
    )
    .bind(integration_id)
    .bind(repository_full_name)
    .fetch_one(pool)
    .await
    .expect("failed to resolve test repository external ID")
}

#[allow(clippy::too_many_arguments)]
fn github_pull_request_payload(
    action: &str,
    repository_full_name: &str,
    repository_id: Option<&str>,
    source_repository_id: Option<&str>,
    target_repository_id: Option<&str>,
    number: i64,
    branch: &str,
    sha: &str,
) -> serde_json::Value {
    serde_json::json!({
        "action": action,
        "number": number,
        "repository": {
            "id": repository_id,
            "full_name": repository_full_name,
        },
        "pull_request": {
            "merged": false,
            "head": {
                "ref": branch,
                "sha": sha,
                "repo": { "id": source_repository_id },
            },
            "base": {
                "ref": "main",
                "repo": { "id": target_repository_id },
            },
        },
        "sender": { "login": "test-user" },
    })
}

#[allow(clippy::too_many_arguments)]
fn gitlab_merge_request_payload(
    action: &str,
    repository_full_name: &str,
    repository_id: &str,
    source_repository_id: Option<&str>,
    target_repository_id: Option<&str>,
    iid: i64,
    branch: &str,
    sha: &str,
    oldrev: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "object_kind": "merge_request",
        "event_type": "merge_request",
        "project": {
            "id": repository_id,
            "path_with_namespace": repository_full_name,
        },
        "object_attributes": {
            "action": action,
            "iid": iid,
            "source_project_id": source_repository_id,
            "target_project_id": target_repository_id,
            "source_branch": branch,
            "target_branch": "main",
            "last_commit": { "id": sha },
            "oldrev": oldrev,
        },
        "user": { "username": "test-user" },
        "created_at": chrono::Utc::now().to_rfc3339(),
    })
}

async fn post_github_webhook(
    app: &axum::Router,
    secret: &str,
    delivery_id: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> hyper::StatusCode {
    let body = serde_json::to_vec(&payload).unwrap();
    let signature = common::github_hmac_signature(&body, secret);
    app.clone()
        .oneshot(
            Request::post("/v1/webhooks/github")
                .header("content-type", "application/json")
                .header("x-hub-signature-256", signature)
                .header("x-github-delivery", delivery_id)
                .header("x-github-event", event_type)
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn post_gitlab_webhook(
    app: &axum::Router,
    secret: &str,
    delivery_id: &str,
    event_type: &str,
    payload: serde_json::Value,
) -> hyper::StatusCode {
    app.clone()
        .oneshot(
            Request::post("/v1/webhooks/gitlab")
                .header("content-type", "application/json")
                .header("x-gitlab-token", secret)
                .header("x-gitlab-event-uuid", delivery_id)
                .header("x-gitlab-event", event_type)
                .body(Body::from(serde_json::to_vec(&payload).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

async fn wait_for_webhook_status(pool: &sqlx::SqlitePool, delivery_id: &str) -> String {
    for _ in 0..80 {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM integration_webhooks WHERE provider_delivery_id = ?1",
        )
        .bind(delivery_id)
        .fetch_optional(pool)
        .await
        .unwrap();
        if let Some(status) = status
            && status != "received"
        {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("webhook {delivery_id} did not finish processing");
}

async fn project_build_count(pool: &sqlx::SqlitePool, project_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM builds WHERE project_id = ?1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn query_value(url: &str, key: &str) -> String {
    url.split_once('?')
        .and_then(|(_, query)| {
            query.split('&').find_map(|pair| {
                pair.strip_prefix(key)
                    .and_then(|value| value.strip_prefix('='))
            })
        })
        .unwrap_or_else(|| panic!("missing {key} in {url}"))
        .to_string()
}

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
async fn test_github_pull_request_revision_trust_policy() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gh-pr-revision-trust-secret";
    let integration_id = common::seed_github_integration(&pool, &user_id, secret).await;
    let repository = "test-org/pr-revision-trust";
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, repository).await;
    let target_id = repository_external_id(&pool, &integration_id, repository).await;

    for (index, action) in ["opened", "reopened", "synchronize"]
        .into_iter()
        .enumerate()
    {
        let delivery_id = format!("gh-pr-allowed-{action}");
        let sha = format!("gh-pr-sha-{index}");
        let payload = github_pull_request_payload(
            action,
            repository,
            Some(&target_id),
            Some(&target_id),
            Some(&target_id),
            index as i64 + 1,
            "feature/same-repository",
            &sha,
        );
        assert_eq!(
            post_github_webhook(&app, secret, &delivery_id, "pull_request", payload).await,
            hyper::StatusCode::OK
        );
        let builds = common::wait_for_builds(&pool, &project_id, index + 1, 2_000).await;
        assert_eq!(builds.len(), index + 1);
        assert_eq!(builds[index]["branch"], "feature/same-repository");
        assert_eq!(builds[index]["commit_sha"], sha);
    }

    let denied = [
        (
            "gh-pr-fork",
            github_pull_request_payload(
                "opened",
                repository,
                Some(&target_id),
                Some("external-fork-repository"),
                Some(&target_id),
                10,
                "feature/fork",
                "gh-pr-fork-sha",
            ),
        ),
        (
            "gh-pr-ambiguous-source",
            github_pull_request_payload(
                "opened",
                repository,
                Some(&target_id),
                None,
                Some(&target_id),
                11,
                "feature/missing-source",
                "gh-pr-missing-source-sha",
            ),
        ),
        (
            "gh-pr-target-mismatch",
            github_pull_request_payload(
                "opened",
                repository,
                Some("different-target-repository"),
                Some("different-target-repository"),
                Some("different-target-repository"),
                12,
                "feature/wrong-target",
                "gh-pr-wrong-target-sha",
            ),
        ),
        (
            "gh-pr-missing-head",
            github_pull_request_payload(
                "opened",
                repository,
                Some(&target_id),
                Some(&target_id),
                Some(&target_id),
                13,
                "feature/missing-head",
                "",
            ),
        ),
        (
            "gh-pr-label-only",
            github_pull_request_payload(
                "labeled",
                repository,
                Some(&target_id),
                Some(&target_id),
                Some(&target_id),
                14,
                "feature/label-only",
                "gh-pr-label-sha",
            ),
        ),
        ("gh-pr-merged", {
            let mut payload = github_pull_request_payload(
                "closed",
                repository,
                Some(&target_id),
                Some(&target_id),
                Some(&target_id),
                15,
                "feature/merged",
                "gh-pr-merged-sha",
            );
            payload["pull_request"]["merged"] = serde_json::Value::Bool(true);
            payload
        }),
    ];

    for (delivery_id, payload) in denied {
        assert_eq!(
            post_github_webhook(&app, secret, delivery_id, "pull_request", payload).await,
            hyper::StatusCode::OK
        );
        assert_eq!(wait_for_webhook_status(&pool, delivery_id).await, "ignored");
        assert_eq!(project_build_count(&pool, &project_id).await, 3);
    }
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
    let external_id = common::seed_gitlab_repository_webhook_secret(
        &pool,
        &integration_id,
        "test-group/test-project",
        secret,
    )
    .await;

    let payload = common::gitlab_push_payload_for_project(
        "test-group/test-project",
        &external_id,
        "main",
        "gl-sha-1",
    );
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
async fn test_gitlab_merge_request_revision_trust_policy() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let secret = "gl-mr-revision-trust-secret";
    let integration_id = common::seed_gitlab_integration(&pool, &user_id, secret).await;
    let repository = "test-group/mr-revision-trust";
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, repository).await;
    let target_id =
        common::seed_gitlab_repository_webhook_secret(&pool, &integration_id, repository, secret)
            .await;

    let allowed = [
        ("open", "gl-mr-open-sha", None),
        ("reopen", "gl-mr-reopen-sha", None),
        ("update", "gl-mr-update-sha", Some("gl-mr-previous-sha")),
    ];
    for (index, (action, sha, oldrev)) in allowed.into_iter().enumerate() {
        let delivery_id = format!("gl-mr-allowed-{action}");
        let payload = gitlab_merge_request_payload(
            action,
            repository,
            &target_id,
            Some(&target_id),
            Some(&target_id),
            index as i64 + 1,
            "feature/same-repository",
            sha,
            oldrev,
        );
        assert_eq!(
            post_gitlab_webhook(&app, secret, &delivery_id, "Merge Request Hook", payload).await,
            hyper::StatusCode::OK
        );
        let builds = common::wait_for_builds(&pool, &project_id, index + 1, 2_000).await;
        assert_eq!(builds.len(), index + 1);
        assert_eq!(builds[index]["branch"], "feature/same-repository");
        assert_eq!(builds[index]["commit_sha"], sha);
    }

    let denied = [
        (
            "gl-mr-fork",
            gitlab_merge_request_payload(
                "open",
                repository,
                &target_id,
                Some("external-fork-project"),
                Some(&target_id),
                10,
                "feature/fork",
                "gl-mr-fork-sha",
                None,
            ),
        ),
        (
            "gl-mr-ambiguous-source",
            gitlab_merge_request_payload(
                "open",
                repository,
                &target_id,
                None,
                Some(&target_id),
                11,
                "feature/missing-source",
                "gl-mr-missing-source-sha",
                None,
            ),
        ),
        (
            "gl-mr-target-mismatch",
            gitlab_merge_request_payload(
                "open",
                repository,
                &target_id,
                Some("different-target-project"),
                Some("different-target-project"),
                12,
                "feature/wrong-target",
                "gl-mr-wrong-target-sha",
                None,
            ),
        ),
        (
            "gl-mr-missing-head",
            gitlab_merge_request_payload(
                "open",
                repository,
                &target_id,
                Some(&target_id),
                Some(&target_id),
                13,
                "feature/missing-head",
                "",
                None,
            ),
        ),
        (
            "gl-mr-update-without-oldrev",
            gitlab_merge_request_payload(
                "update",
                repository,
                &target_id,
                Some(&target_id),
                Some(&target_id),
                14,
                "feature/metadata-update",
                "gl-mr-metadata-sha",
                None,
            ),
        ),
        (
            "gl-mr-update-with-unchanged-head",
            gitlab_merge_request_payload(
                "update",
                repository,
                &target_id,
                Some(&target_id),
                Some(&target_id),
                15,
                "feature/unchanged-head",
                "gl-mr-unchanged-sha",
                Some("gl-mr-unchanged-sha"),
            ),
        ),
        (
            "gl-mr-close",
            gitlab_merge_request_payload(
                "close",
                repository,
                &target_id,
                Some(&target_id),
                Some(&target_id),
                16,
                "feature/closed",
                "gl-mr-close-sha",
                None,
            ),
        ),
        (
            "gl-mr-merge",
            gitlab_merge_request_payload(
                "merge",
                repository,
                &target_id,
                Some(&target_id),
                Some(&target_id),
                17,
                "feature/merged",
                "gl-mr-merge-sha",
                None,
            ),
        ),
    ];

    for (delivery_id, payload) in denied {
        assert_eq!(
            post_gitlab_webhook(&app, secret, delivery_id, "Merge Request Hook", payload,).await,
            hyper::StatusCode::OK
        );
        assert_eq!(wait_for_webhook_status(&pool, delivery_id).await, "ignored");
        assert_eq!(project_build_count(&pool, &project_id).await, 3);
    }
}

#[tokio::test]
async fn test_gitlab_repository_token_cannot_authorize_sibling_repository() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let integration_id = common::seed_gitlab_integration(&pool, &user_id, "unused").await;
    let (project_a, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "group/repo-a").await;
    let (project_b, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "group/repo-b").await;
    let external_a = common::seed_gitlab_repository_webhook_secret(
        &pool,
        &integration_id,
        "group/repo-a",
        "repo-a-token",
    )
    .await;
    let external_b = common::seed_gitlab_repository_webhook_secret(
        &pool,
        &integration_id,
        "group/repo-b",
        "repo-b-token",
    )
    .await;

    let sibling = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", "repo-a-token")
        .header("x-gitlab-event-uuid", "gl-sibling-rejected")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(
            serde_json::to_vec(&common::gitlab_push_payload_for_project(
                "group/repo-b",
                &external_b,
                "main",
                "sibling-sha",
            ))
            .unwrap(),
        ))
        .unwrap();
    assert_eq!(app.clone().oneshot(sibling).await.unwrap().status(), 401);

    // The payload path is untrusted routing data. A valid repository A token
    // and immutable A project ID must route to A even when the path claims B.
    let spoofed_path = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", "repo-a-token")
        .header("x-gitlab-event-uuid", "gl-sibling-path-spoof")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(
            serde_json::to_vec(&common::gitlab_push_payload_for_project(
                "group/repo-b",
                &external_a,
                "main",
                "trusted-a-sha",
            ))
            .unwrap(),
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(spoofed_path).await.unwrap().status(),
        200
    );

    let builds_a = common::wait_for_builds(&pool, &project_a, 1, 2_000).await;
    assert_eq!(builds_a.len(), 1);
    assert_eq!(builds_a[0]["commit_sha"], "trusted-a-sha");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let builds_b = common::wait_for_builds(&pool, &project_b, 0, 200).await;
    assert!(
        builds_b.is_empty(),
        "sibling repository must not receive a build"
    );
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
    let external_id = common::seed_gitlab_repository_webhook_secret(
        &pool,
        &integration_id,
        "group/idemp-proj",
        secret,
    )
    .await;

    let payload = common::gitlab_push_payload_for_project(
        "group/idemp-proj",
        &external_id,
        "dev",
        "gl-sha-idemp",
    );
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
    let external_id = common::seed_gitlab_repository_webhook_secret(
        &pool,
        &integration_id,
        "group/fallback-proj",
        secret,
    )
    .await;
    let body = serde_json::to_vec(&common::gitlab_push_payload_for_project(
        "group/fallback-proj",
        &external_id,
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
    let integration_id = common::seed_gitlab_integration(&pool, &user_id, old_secret).await;
    let (project_id, _) =
        common::seed_project_chain(&pool, &integration_id, &user_id, "group/rotate-proj").await;
    let external_id = common::seed_gitlab_repository_webhook_secret(
        &pool,
        &integration_id,
        "group/rotate-proj",
        old_secret,
    )
    .await;

    // First webhook uses old secret and warms the in-process cache.
    let payload1 = common::gitlab_push_payload_for_project(
        "group/rotate-proj",
        &external_id,
        "main",
        "gl-sha-old",
    );
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

    let repository_id: String = sqlx::query_scalar(
        "SELECT r.id FROM integration_repositories r \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         WHERE inst.integration_id = ?1 AND r.full_name = 'group/rotate-proj'",
    )
    .bind(&integration_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let session = common::create_session_token(&pool, &user_id).await;
    let rotate = Request::post(format!(
        "/v1/integration-repositories/{repository_id}/gitlab-webhook-secret"
    ))
    .header("authorization", format!("Bearer {session}"))
    .body(Body::empty())
    .unwrap();
    let rotate_response = app.clone().oneshot(rotate).await.unwrap();
    assert_eq!(rotate_response.status(), 200);
    let rotate_json = common::body_json(rotate_response.into_body()).await;
    let new_secret = rotate_json["webhook_secret"].as_str().unwrap();

    let old_replay = Request::post("/v1/webhooks/gitlab")
        .header("content-type", "application/json")
        .header("x-gitlab-token", old_secret)
        .header("x-gitlab-event-uuid", "gl-delivery-rotate-old-replay")
        .header("x-gitlab-event", "Push Hook")
        .body(Body::from(
            serde_json::to_vec(&common::gitlab_push_payload_for_project(
                "group/rotate-proj",
                &external_id,
                "main",
                "gl-sha-old-replay",
            ))
            .unwrap(),
        ))
        .unwrap();
    assert_eq!(app.clone().oneshot(old_replay).await.unwrap().status(), 401);

    // Second webhook uses new secret. It should still succeed without restart
    // by forcing a cache refresh after the initial cached-token miss.
    let payload2 = common::gitlab_push_payload_for_project(
        "group/rotate-proj",
        &external_id,
        "main",
        "gl-sha-new",
    );
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
    let integration_id = common::seed_gitlab_integration(&pool, &user_id, "correct-token").await;
    common::seed_project_chain(&pool, &integration_id, &user_id, "group/repo").await;
    let external_id = common::seed_gitlab_repository_webhook_secret(
        &pool,
        &integration_id,
        "group/repo",
        "correct-token",
    )
    .await;

    let payload =
        common::gitlab_push_payload_for_project("group/repo", &external_id, "main", "sha1");
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
    let integration_id = common::seed_gitlab_integration(&pool, &user_id, secret).await;
    common::seed_project_chain(&pool, &integration_id, &user_id, "group/stale-repo").await;
    let external_id = common::seed_gitlab_repository_webhook_secret(
        &pool,
        &integration_id,
        "group/stale-repo",
        secret,
    )
    .await;

    // Create a payload with a timestamp 6 minutes in the past
    let stale_time = chrono::Utc::now() - chrono::Duration::seconds(360);
    let payload = serde_json::json!({
        "ref": "refs/heads/main",
        "checkout_sha": "stale-sha",
        "project": {
            "id": external_id,
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

// ── SCM callback tests ────────────────────────────────────────

#[tokio::test]
async fn test_github_callback_revalidates_initiator_and_rejects_replay() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let session = common::create_session_token(&pool, &user_id).await;
    let start = Request::post("/v1/integrations/github/start")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {session}"))
        .body(Body::from(
            serde_json::json!({
                "webhook_url": "https://oore.example.com/v1/webhooks/github",
                "redirect_url": "http://localhost:3000/settings/integrations"
            })
            .to_string(),
        ))
        .unwrap();
    let start_response = app.clone().oneshot(start).await.unwrap();
    assert_eq!(start_response.status(), 200);
    let start_json = common::body_json(start_response.into_body()).await;
    let state = query_value(start_json["create_url"].as_str().unwrap(), "state");

    sqlx::query("UPDATE users SET status = 'disabled' WHERE id = ?1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

    let callback_url = format!("/v1/integrations/github/callback?code=test-code&state={state}");
    let first = app
        .clone()
        .oneshot(Request::get(&callback_url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let first_body = common::body_string(first.into_body()).await;
    assert!(
        first_body.contains("no longer authorized"),
        "unexpected first callback response: {first_body}"
    );

    let replay = app
        .oneshot(Request::get(&callback_url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let replay_body = common::body_string(replay.into_body()).await;
    assert!(replay_body.contains("setup link has expired"));
}

#[tokio::test]
async fn test_github_installed_rejects_bare_known_installation_id() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let integration_id = common::seed_github_integration(&pool, &user_id, "github-secret").await;
    let now = common::now_unix();
    sqlx::query(
        "INSERT INTO integration_installations \
         (id, integration_id, external_id, account_name, account_type, created_at, updated_at) \
         VALUES ('known-installation', ?1, '12345', 'test-org', 'Organization', ?2, ?2)",
    )
    .bind(&integration_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::get("/v1/integrations/github/installed?installation_id=12345")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = common::body_string(response.into_body()).await;
    assert!(body.contains("github=success"));
    assert!(!body.contains(&integration_id));
}

#[tokio::test]
async fn test_gitlab_callback_rejects_unissued_state() {
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

    let state = common::seal_gitlab_oauth_state(
        &integration_id,
        "http://localhost:3000/settings/integrations",
    );

    let req = Request::get(format!(
        "/v1/integrations/gitlab/callback?code=test-code&state={}",
        state
    ))
    .body(Body::empty())
    .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = common::body_string(resp.into_body()).await;
    assert!(body.contains("authorization link has expired"));
}

#[tokio::test]
async fn test_gitlab_callback_revalidates_initiator_and_rejects_replay() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let session = common::create_session_token(&pool, &user_id).await;
    let integration_id = common::seed_gitlab_oauth_integration(
        &pool,
        &user_id,
        "webhook-secret",
        "test-client-id",
        "test-client-secret",
    )
    .await;
    let authorize = Request::post("/v1/integrations/gitlab/authorize")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {session}"))
        .body(Body::from(
            serde_json::json!({
                "integration_id": integration_id,
                "redirect_url": "http://localhost:3000/settings/integrations"
            })
            .to_string(),
        ))
        .unwrap();
    let authorize_response = app.clone().oneshot(authorize).await.unwrap();
    assert_eq!(authorize_response.status(), 200);
    let authorize_json = common::body_json(authorize_response.into_body()).await;
    let state = query_value(authorize_json["authorize_url"].as_str().unwrap(), "state");

    sqlx::query("UPDATE users SET role = 'qa_viewer' WHERE id = ?1")
        .bind(&user_id)
        .execute(&pool)
        .await
        .unwrap();

    let callback_url = format!("/v1/integrations/gitlab/callback?code=test-code&state={state}");
    let first = app
        .clone()
        .oneshot(Request::get(&callback_url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let first_body = common::body_string(first.into_body()).await;
    assert!(
        first_body.contains("no longer authorized"),
        "unexpected first callback response: {first_body}"
    );

    let replay = app
        .oneshot(Request::get(&callback_url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let replay_body = common::body_string(replay.into_body()).await;
    assert!(replay_body.contains("authorization link has expired"));
}

#[tokio::test]
async fn test_gitlab_authorize_rejects_persisted_cleartext_origin() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let app = common::create_test_app(&db_path).await;
    let pool = common::connect_pool(&db_path).await;
    common::set_runtime_mode(&pool, "remote").await;

    let user_id = common::seed_test_user(&pool).await;
    let session = common::create_session_token(&pool, &user_id).await;
    let integration_id = common::seed_gitlab_oauth_integration(
        &pool,
        &user_id,
        "webhook-secret",
        "test-client-id",
        "test-client-secret",
    )
    .await;
    sqlx::query("UPDATE integrations SET host_url = 'http://gitlab.internal' WHERE id = ?1")
        .bind(&integration_id)
        .execute(&pool)
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::post("/v1/integrations/gitlab/authorize")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {session}"))
                .body(Body::from(
                    serde_json::json!({
                        "integration_id": integration_id,
                        "redirect_url": "http://localhost:3000/settings/integrations"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let json = common::body_json(response.into_body()).await;
    assert_eq!(json["code"], "invalid_input");
}
