#![allow(dead_code)]

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use oored::build_test_router;
use oored::store::SetupStore;
use ring::hmac;
use sqlx::SqlitePool;
use uuid::Uuid;

/// Fixed test encryption key (32 bytes).
pub const TEST_ENCRYPTION_KEY: [u8; 32] = [0x42u8; 32];

/// Current UNIX timestamp in seconds.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Create a test app backed by a SQLite database at the given path.
pub async fn create_test_app(db_path: &Path) -> Router {
    let store = SetupStore::connect(db_path.to_path_buf())
        .await
        .expect("failed to connect to test database");
    store
        .init_if_missing()
        .await
        .expect("failed to init database");
    build_test_router(store, TEST_ENCRYPTION_KEY.to_vec()).await
}

/// Create a test app with an externally reachable public URL loaded into runtime state.
pub async fn create_test_app_with_public_url(db_path: &Path, public_url: &str) -> Router {
    create_test_app_with_network_urls(db_path, Some(public_url), None).await
}

pub async fn create_test_app_with_network_urls(
    db_path: &Path,
    public_url: Option<&str>,
    artifact_delivery_url: Option<&str>,
) -> Router {
    let store = SetupStore::connect(db_path.to_path_buf())
        .await
        .expect("failed to connect to test database");
    store
        .init_if_missing()
        .await
        .expect("failed to init database");
    let now = now_unix();
    sqlx::query(
        "INSERT INTO external_access_network_settings \
         (id, public_url, artifact_delivery_url, allowed_origins_json, created_at, updated_at) \
         VALUES (1, ?1, ?2, ?3, ?4, ?4)",
    )
    .bind(public_url)
    .bind(artifact_delivery_url)
    .bind(serde_json::to_string(&public_url.into_iter().collect::<Vec<_>>()).unwrap())
    .bind(now)
    .execute(store.pool())
    .await
    .expect("failed to seed public URL");
    build_test_router(store, TEST_ENCRYPTION_KEY.to_vec()).await
}

/// Connect to the test database at `path` and return the pool.
pub async fn connect_pool(path: &Path) -> SqlitePool {
    let store = SetupStore::connect(path.to_path_buf())
        .await
        .expect("failed to connect to test database");
    store.pool().clone()
}

pub async fn set_runtime_mode(pool: &SqlitePool, mode: &str) {
    let now = now_unix();
    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, runtime_mode, created_at, updated_at) \
         VALUES (1, 'keychain', ?1, ?2, ?2) \
         ON CONFLICT(id) DO UPDATE SET runtime_mode = excluded.runtime_mode, updated_at = excluded.updated_at",
    )
    .bind(mode)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to set runtime mode");
}

/// Parse a response body as JSON value.
pub async fn body_json(body: axum::body::Body) -> serde_json::Value {
    let bytes = http_body_util::BodyExt::collect(body)
        .await
        .expect("failed to read response body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("failed to parse response JSON")
}

/// Seed a test user and return the user_id.
pub async fn seed_test_user(pool: &SqlitePool) -> String {
    let user_id = Uuid::new_v4().to_string();
    let now = now_unix();
    sqlx::query(
        "INSERT OR IGNORE INTO users (id, email, oidc_subject, display_name, role, status, created_at, updated_at) \
         VALUES (?1, 'test@example.com', 'test-subject', 'Test User', 'owner', 'active', ?2, ?2)",
    )
    .bind(&user_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed test user");
    user_id
}

/// Create a session token for an existing test user.
pub async fn create_session_token(pool: &SqlitePool, user_id: &str) -> String {
    let token = oored::token::generate_session_token();
    let now = now_unix();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(oored::token::hash_token(&token))
    .bind(user_id)
    .bind(now)
    .bind(now + 86_400)
    .execute(pool)
    .await
    .expect("failed to create test session");
    token
}

/// Seed a GitHub integration with a known webhook secret and return the integration_id.
pub async fn seed_github_integration(
    pool: &SqlitePool,
    user_id: &str,
    webhook_secret: &str,
) -> String {
    let integration_id = Uuid::new_v4().to_string();
    let now = now_unix();

    sqlx::query(
        "INSERT INTO integrations (id, provider, host_url, auth_mode, status, display_name, created_by, created_at, updated_at) \
         VALUES (?1, 'github', 'https://github.com', 'github_app', 'active', 'Test GitHub', ?2, ?3, ?3)",
    )
    .bind(&integration_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed integration");

    // Store webhook secret (encrypted)
    let encrypted = oored::crypto::encrypt(webhook_secret, &TEST_ENCRYPTION_KEY)
        .expect("failed to encrypt webhook secret");

    sqlx::query(
        "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
         VALUES (?1, ?2, 'webhook_secret', ?3, ?4, ?4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&integration_id)
    .bind(&encrypted)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed webhook secret");

    integration_id
}

/// Seed a GitLab integration with a known webhook secret and return the integration_id.
pub async fn seed_gitlab_integration(
    pool: &SqlitePool,
    user_id: &str,
    webhook_secret: &str,
) -> String {
    let integration_id = Uuid::new_v4().to_string();
    let now = now_unix();

    sqlx::query(
        "INSERT INTO integrations (id, provider, host_url, auth_mode, status, display_name, created_by, created_at, updated_at) \
         VALUES (?1, 'gitlab', 'https://gitlab.com', 'personal_token', 'active', 'Test GitLab', ?2, ?3, ?3)",
    )
    .bind(&integration_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed GitLab integration");

    let encrypted = oored::crypto::encrypt(webhook_secret, &TEST_ENCRYPTION_KEY)
        .expect("failed to encrypt webhook secret");

    sqlx::query(
        "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
         VALUES (?1, ?2, 'webhook_secret', ?3, ?4, ?4)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&integration_id)
    .bind(&encrypted)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed GitLab webhook secret");

    integration_id
}

/// Seed a full chain: installation → repository → project → pipeline.
/// Returns `(project_id, pipeline_id)`.
pub async fn seed_project_chain(
    pool: &SqlitePool,
    integration_id: &str,
    user_id: &str,
    repo_full_name: &str,
) -> (String, String) {
    let now = now_unix();
    let installation_id = Uuid::new_v4().to_string();
    let repo_id = Uuid::new_v4().to_string();
    let project_id = Uuid::new_v4().to_string();
    let pipeline_id = Uuid::new_v4().to_string();

    // Installation — use unique external_id based on installation UUID
    sqlx::query(
        "INSERT INTO integration_installations (id, integration_id, external_id, account_name, account_type, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'test-org', 'Organization', ?4, ?4)",
    )
    .bind(&installation_id)
    .bind(integration_id)
    .bind(format!("ext-install-{}", &installation_id[..8]))
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed installation");

    // Repository — use unique external_id based on repo UUID
    sqlx::query(
        "INSERT INTO integration_repositories (id, installation_id, external_id, full_name, default_branch, is_private, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, 'main', 0, ?5, ?5)",
    )
    .bind(&repo_id)
    .bind(&installation_id)
    .bind(format!("ext-repo-{}", &repo_id[..8]))
    .bind(repo_full_name)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed repository");

    // Project
    sqlx::query(
        "INSERT INTO projects (id, name, repository_id, created_by, created_at, updated_at) \
         VALUES (?1, 'Test Project', ?2, ?3, ?4, ?4)",
    )
    .bind(&project_id)
    .bind(&repo_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed project");

    // Pipeline
    sqlx::query(
        "INSERT INTO pipelines (id, project_id, name, config_path, trigger_config, concurrency, enabled, created_at, updated_at) \
         VALUES (?1, ?2, 'Default', '.oore.yml', '{}', '{}', 1, ?3, ?3)",
    )
    .bind(&pipeline_id)
    .bind(&project_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed pipeline");

    (project_id, pipeline_id)
}

/// Seed a webhook record in `integration_webhooks` and return the webhook_id.
pub async fn seed_webhook_record(
    pool: &SqlitePool,
    integration_id: &str,
    delivery_id: &str,
) -> String {
    let webhook_id = Uuid::new_v4().to_string();
    let now = now_unix();
    sqlx::query(
        "INSERT INTO integration_webhooks (id, integration_id, provider_delivery_id, event_type, payload, status, received_at) \
         VALUES (?1, ?2, ?3, 'push', '{}', 'received', ?4)",
    )
    .bind(&webhook_id)
    .bind(integration_id)
    .bind(delivery_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed webhook record");
    webhook_id
}

/// Parse a response body as a UTF-8 string.
pub async fn body_string(body: axum::body::Body) -> String {
    let bytes = http_body_util::BodyExt::collect(body)
        .await
        .expect("failed to read response body")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("response body is not valid UTF-8")
}

/// Seed an **inactive** GitLab OAuth integration with client credentials.
/// Returns the integration_id.
pub async fn seed_gitlab_oauth_integration(
    pool: &SqlitePool,
    user_id: &str,
    webhook_secret: &str,
    client_id: &str,
    client_secret: &str,
) -> String {
    let integration_id = Uuid::new_v4().to_string();
    let now = now_unix();

    sqlx::query(
        "INSERT INTO integrations (id, provider, host_url, auth_mode, status, display_name, created_by, created_at, updated_at) \
         VALUES (?1, 'gitlab', 'https://gitlab.com', 'oauth_app', 'inactive', 'Test GitLab OAuth', ?2, ?3, ?3)",
    )
    .bind(&integration_id)
    .bind(user_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("failed to seed GitLab OAuth integration");

    // Store webhook secret, client_id, client_secret
    for (cred_type, value) in [
        ("webhook_secret", webhook_secret),
        ("oauth_client_id", client_id),
        ("oauth_client_secret", client_secret),
    ] {
        let encrypted = oored::crypto::encrypt(value, &TEST_ENCRYPTION_KEY)
            .expect("failed to encrypt credential");

        sqlx::query(
            "INSERT INTO integration_credentials (id, integration_id, credential_type, encrypted_value, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&integration_id)
        .bind(cred_type)
        .bind(&encrypted)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to seed OAuth credential");
    }

    integration_id
}

/// Create a sealed GitLab OAuth state token (mirrors the server's `seal_gitlab_state`).
pub fn seal_gitlab_oauth_state(integration_id: &str, redirect_url: &str) -> String {
    let state = serde_json::json!({
        "integration_id": integration_id,
        "redirect_url": redirect_url,
        "created_at": now_unix(),
    });
    let json = serde_json::to_string(&state).unwrap();
    let encrypted =
        oored::crypto::encrypt(&json, &TEST_ENCRYPTION_KEY).expect("failed to encrypt state token");
    urlencoding::encode(&encrypted).into_owned()
}

/// Compute a valid GitHub HMAC-SHA256 signature for a body.
pub fn github_hmac_signature(body: &[u8], secret: &str) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let tag = hmac::sign(&key, body);
    format!("sha256={}", hex::encode(tag.as_ref()))
}

/// Minimal valid GitHub push webhook payload.
pub fn github_push_payload(repo: &str, branch: &str, sha: &str) -> serde_json::Value {
    serde_json::json!({
        "ref": format!("refs/heads/{branch}"),
        "after": sha,
        "repository": {
            "full_name": repo,
            "html_url": format!("https://github.com/{repo}")
        },
        "sender": {
            "login": "test-user"
        }
    })
}

/// Minimal valid GitLab push webhook payload.
pub fn gitlab_push_payload(repo: &str, branch: &str, sha: &str) -> serde_json::Value {
    serde_json::json!({
        "ref": format!("refs/heads/{branch}"),
        "checkout_sha": sha,
        "project": {
            "path_with_namespace": repo,
            "web_url": format!("https://gitlab.com/{repo}")
        },
        "user_username": "test-user",
        "created_at": chrono::Utc::now().to_rfc3339()
    })
}

/// Wait for builds to appear for a project, polling every 50ms.
pub async fn wait_for_builds(
    pool: &SqlitePool,
    project_id: &str,
    expected: usize,
    timeout_ms: u64,
) -> Vec<serde_json::Value> {
    let start = std::time::Instant::now();
    loop {
        let rows =
            sqlx::query("SELECT * FROM builds WHERE project_id = ?1 ORDER BY build_number ASC")
                .bind(project_id)
                .fetch_all(pool)
                .await
                .unwrap_or_default();

        if rows.len() >= expected {
            return rows
                .iter()
                .map(|r| {
                    use sqlx::Row;
                    serde_json::json!({
                        "id": r.get::<String, _>("id"),
                        "build_number": r.get::<i64, _>("build_number"),
                        "status": r.get::<String, _>("status"),
                        "trigger_type": r.get::<String, _>("trigger_type"),
                        "trigger_event": r.get::<Option<String>, _>("trigger_event"),
                        "branch": r.get::<Option<String>, _>("branch"),
                        "commit_sha": r.get::<Option<String>, _>("commit_sha"),
                    })
                })
                .collect();
        }

        if start.elapsed().as_millis() as u64 > timeout_ms {
            return rows
                .iter()
                .map(|r| {
                    use sqlx::Row;
                    serde_json::json!({
                        "id": r.get::<String, _>("id"),
                        "build_number": r.get::<i64, _>("build_number"),
                    })
                })
                .collect();
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}
