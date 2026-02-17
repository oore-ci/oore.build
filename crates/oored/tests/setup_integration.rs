// Integration tests require the test-support feature to access build_test_router.
// Run with: cargo test -p oored --features test-support
#![cfg(feature = "test-support")]

use std::net::SocketAddr;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use http_body_util::BodyExt;
use hyper::Request;
use oore_contract::{BootstrapTokenRecord, SetupSessionRecord, SetupState};
use oored::build_test_router;
use oored::store::SetupStore;
use oored::token::{generate_token, hash_token};

/// Fixed test encryption key (32 bytes).
const TEST_ENCRYPTION_KEY: [u8; 32] = [0x42u8; 32];
use serde_json::{Value, json};
use tower::ServiceExt;

// ── Helpers ──────────────────────────────────────────────────────

/// Current UNIX timestamp in seconds.
fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Create a test app backed by a SQLite database at the given path.
/// Initializes the store (creates the database and initial row if missing)
/// and builds the router.
async fn create_test_app(db_path: &Path) -> Router {
    let store = SetupStore::connect(db_path.to_path_buf())
        .await
        .expect("failed to connect to test database");
    store
        .init_if_missing()
        .await
        .expect("failed to init database");
    build_test_router(store, TEST_ENCRYPTION_KEY.to_vec()).await
}

/// Connect to the test database at `path` and return the store.
async fn connect_store(path: &Path) -> SetupStore {
    SetupStore::connect(path.to_path_buf())
        .await
        .expect("failed to connect to test database")
}

/// Seed the database at `path` with a valid bootstrap token that
/// expires 15 minutes from now. Returns the plaintext token.
async fn seed_bootstrap_token(path: &Path) -> String {
    let store = connect_store(path).await;
    let mut sf = store.load().await.expect("failed to load state");

    let plaintext = generate_token();
    let hash = hash_token(&plaintext);

    sf.bootstrap_token = Some(BootstrapTokenRecord {
        hash,
        expires_at: now_unix() + 15 * 60, // 15 minutes
        consumed_at: None,
    });

    store.save(&sf).await.expect("failed to save state");
    plaintext
}

/// Seed the database with an *already-expired* bootstrap token.
/// Returns the plaintext token (which would have been valid).
async fn seed_expired_bootstrap_token(path: &Path) -> String {
    let store = connect_store(path).await;
    let mut sf = store.load().await.expect("failed to load state");

    let plaintext = generate_token();
    let hash = hash_token(&plaintext);

    sf.bootstrap_token = Some(BootstrapTokenRecord {
        hash,
        expires_at: now_unix() - 1, // already expired
        consumed_at: None,
    });

    store.save(&sf).await.expect("failed to save state");
    plaintext
}

/// Seed the database with a consumed bootstrap token.
/// Returns the plaintext token.
async fn seed_consumed_bootstrap_token(path: &Path) -> String {
    let store = connect_store(path).await;
    let mut sf = store.load().await.expect("failed to load state");

    let plaintext = generate_token();
    let hash = hash_token(&plaintext);

    sf.bootstrap_token = Some(BootstrapTokenRecord {
        hash,
        expires_at: now_unix() + 15 * 60,
        consumed_at: Some(now_unix() - 10), // consumed 10 seconds ago
    });

    store.save(&sf).await.expect("failed to save state");
    plaintext
}

/// Seed a valid session token into the database.
/// Returns the plaintext session token.
async fn seed_session_token(path: &Path) -> String {
    let store = connect_store(path).await;
    let mut sf = store.load().await.expect("failed to load state");

    let plaintext = generate_token();
    let hash = hash_token(&plaintext);

    sf.setup_session = Some(SetupSessionRecord {
        hash,
        expires_at: now_unix() + 30 * 60, // 30 minutes
    });

    store.save(&sf).await.expect("failed to save state");
    plaintext
}

/// Seed an *expired* session token into the database.
/// Returns the plaintext session token.
async fn seed_expired_session_token(path: &Path) -> String {
    let store = connect_store(path).await;
    let mut sf = store.load().await.expect("failed to load state");

    let plaintext = generate_token();
    let hash = hash_token(&plaintext);

    sf.setup_session = Some(SetupSessionRecord {
        hash,
        expires_at: now_unix() - 1, // already expired
    });

    store.save(&sf).await.expect("failed to save state");
    plaintext
}

/// Force the setup state to a specific value.
async fn set_state(path: &Path, state: SetupState) {
    let store = connect_store(path).await;
    let mut sf = store.load().await.expect("failed to load state");
    sf.setup_state = state;
    store.save(&sf).await.expect("failed to save state");
}

async fn set_runtime_and_remote_auth_mode(path: &Path, runtime_mode: &str, remote_auth_mode: &str) {
    let store = connect_store(path).await;
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
    .execute(store.pool())
    .await
    .expect("failed to set runtime/auth mode");
}

/// Extract the JSON body from a response.
async fn body_json(response: axum::response::Response) -> Value {
    let body = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&body).unwrap()
}

/// Run the full happy-path setup flow starting from bootstrap_pending.
/// Returns the session token and the final app (after setup is complete).
///
/// This helper is used by tests that need a fully-configured instance.
async fn run_full_setup(path: &Path) -> String {
    let app = create_test_app(path).await;
    let token = seed_bootstrap_token(path).await;

    // Step 1: Verify bootstrap token
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/bootstrap-token/verify")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({"token": token})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    let session_token = body["session_token"].as_str().unwrap().to_string();

    // Step 2: Configure OIDC
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://accounts.google.com",
                        "client_id": "test-client-id",
                        "client_secret": "test-secret"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert_eq!(body["state"], "idp_configured");

    // Step 3a: Start owner OIDC flow
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/start-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(
                        &json!({"redirect_uri": "http://localhost:3000/auth/callback"}),
                    )
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    let oidc_state = body["state"].as_str().unwrap().to_string();

    // Step 3b: Verify owner OIDC callback
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/verify-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "code": "test-auth-code",
                        "state": oidc_state
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert_eq!(body["state"], "owner_created");

    // Step 4: Complete setup
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/complete")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert_eq!(body["state"], "ready");

    session_token
}

// ── Happy path tests ─────────────────────────────────────────────

#[tokio::test]
async fn test_setup_status_returns_bootstrap_pending() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/public/setup-status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let body = body_json(resp).await;
    assert_eq!(body["state"], "bootstrap_pending");
    assert_eq!(body["setup_mode"], true);
    assert_eq!(body["is_configured"], false);
    // instance_id should be a non-empty string
    assert!(!body["instance_id"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_full_setup_flow() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let token = seed_bootstrap_token(&db_path).await;

    // Step 1: Verify bootstrap token → get session token
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/bootstrap-token/verify")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({"token": token})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    let session_token = body["session_token"].as_str().unwrap().to_string();
    assert!(body["expires_at"].as_i64().unwrap() > now_unix());

    // Step 2: Configure OIDC
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://accounts.google.com",
                        "client_id": "test-client-id",
                        "client_secret": "test-secret"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert_eq!(body["state"], "idp_configured");

    // Verify status updated
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/public/setup-status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["state"], "idp_configured");

    // Step 3a: Start owner OIDC flow
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/start-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(
                        &json!({"redirect_uri": "http://localhost:3000/auth/callback"}),
                    )
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    let oidc_state = body["state"].as_str().unwrap().to_string();
    assert!(!oidc_state.is_empty());
    assert!(
        body["authorization_url"]
            .as_str()
            .unwrap()
            .contains("oauth2")
    );

    // Step 3b: Verify owner OIDC callback (test mode uses mock claims)
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/verify-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "code": "test-auth-code",
                        "state": oidc_state
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert_eq!(body["state"], "owner_created");
    assert_eq!(body["owner_email"], "admin@example.com");
    assert!(
        body["oidc_subject"]
            .as_str()
            .unwrap()
            .starts_with("test-subject-")
    );

    // Step 4: Complete setup
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/complete")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert_eq!(body["state"], "ready");
    assert!(!body["instance_id"].as_str().unwrap().is_empty());

    // Verify final status
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/public/setup-status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = body_json(resp).await;
    assert_eq!(body["state"], "ready");
    assert_eq!(body["setup_mode"], false);
    assert_eq!(body["is_configured"], true);
}

#[tokio::test]
async fn test_healthz() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert_eq!(body["ok"], true);
}

// ── Bootstrap token edge cases ──────────────────────────────────

#[tokio::test]
async fn test_verify_expired_bootstrap_token() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let token = seed_expired_bootstrap_token(&db_path).await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/bootstrap-token/verify")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({"token": token})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 410); // Gone
    let body = body_json(resp).await;
    assert_eq!(body["code"], "token_expired");
}

#[tokio::test]
async fn test_verify_consumed_bootstrap_token() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let token = seed_consumed_bootstrap_token(&db_path).await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/bootstrap-token/verify")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({"token": token})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 410); // Gone
    let body = body_json(resp).await;
    assert_eq!(body["code"], "token_consumed");
}

#[tokio::test]
async fn test_verify_invalid_bootstrap_token() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let _valid_token = seed_bootstrap_token(&db_path).await;

    // Send a completely wrong token
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/bootstrap-token/verify")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({"token": "totally-wrong-token"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "invalid_token");
}

#[tokio::test]
async fn test_verify_no_bootstrap_token_record() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    // Do NOT seed a bootstrap token — the database has no token record

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/bootstrap-token/verify")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({"token": "anything"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 500);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "no_bootstrap_token");
}

// ── Session token edge cases ────────────────────────────────────

#[tokio::test]
async fn test_configure_oidc_missing_auth_header() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;

    // State is bootstrap_pending (correct for OIDC configure), but no auth header
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://example.com",
                        "client_id": "test"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "missing_auth");
}

#[tokio::test]
async fn test_configure_oidc_invalid_session() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    // Seed a valid session so the "no_session" check passes, but send wrong token
    let _real_session = seed_session_token(&db_path).await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer wrong-session-token")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://example.com",
                        "client_id": "test"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "invalid_session");
}

#[tokio::test]
async fn test_configure_oidc_expired_session() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let session_token = seed_expired_session_token(&db_path).await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://example.com",
                        "client_id": "test"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 401);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "session_expired");
}

#[tokio::test]
async fn test_start_owner_oidc_accepts_http_local_network_redirect_uri() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let session_token = seed_session_token(&db_path).await;

    // Move to idp_configured with a valid OIDC config.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://accounts.google.com",
                        "client_id": "test-client-id"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/start-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(
                        &json!({"redirect_uri": "http://jarvis.local:4173/auth/callback"}),
                    )
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert!(
        body["authorization_url"]
            .as_str()
            .unwrap()
            .starts_with("https://")
    );
    assert!(!body["state"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_start_owner_oidc_rejects_http_public_redirect_uri() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let session_token = seed_session_token(&db_path).await;

    // Move to idp_configured with a valid OIDC config.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://accounts.google.com",
                        "client_id": "test-client-id"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/start-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(
                        &json!({"redirect_uri": "http://example.com/auth/callback"}),
                    )
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "invalid_redirect_uri");
    assert_eq!(body["error"], "public redirect_uri must use https scheme");
}

// ── State machine enforcement ───────────────────────────────────

#[tokio::test]
async fn test_configure_oidc_allowed_in_idp_configured_and_clears_pending_auth() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let session_token = seed_session_token(&db_path).await;

    // Configure once from bootstrap_pending -> idp_configured.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://accounts.google.com",
                        "client_id": "initial-client-id"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Create a pending auth entry with the original OIDC configuration.
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/start-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(
                        &json!({"redirect_uri": "http://localhost:3000/auth/callback"}),
                    )
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    let stale_state = body["state"]
        .as_str()
        .expect("missing OIDC state")
        .to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://accounts.google.com",
                        "client_id": "updated-client-id"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert_eq!(body["state"], "idp_configured");

    // The stale pending state should be invalid after reconfiguration.
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/verify-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "code": "test-auth-code",
                        "state": stale_state
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "invalid_state");
}

#[tokio::test]
async fn test_configure_oidc_rejected_in_owner_created_state() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let session_token = seed_session_token(&db_path).await;

    set_state(&db_path, SetupState::OwnerCreated).await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://example.com",
                        "client_id": "test"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "invalid_state");
}

#[tokio::test]
async fn test_complete_setup_wrong_state() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let session_token = seed_session_token(&db_path).await;

    // Move state to idp_configured — complete requires owner_created
    set_state(&db_path, SetupState::IdpConfigured).await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/complete")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "invalid_state");
}

#[tokio::test]
async fn test_setup_owner_claim_trusted_proxy_requires_configuration() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");
    let app = create_test_app(&db_path).await;
    let session_token = seed_session_token(&db_path).await;

    set_state(&db_path, SetupState::IdpConfigured).await;
    set_runtime_and_remote_auth_mode(&db_path, "remote", "trusted_proxy").await;

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/claim-trusted-proxy")
                .header("Authorization", format!("Bearer {}", session_token))
                .header("x-warpgate-username", "owner@example.com")
                .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 41234))))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "trusted_proxy_not_configured");
}

// ── Ready-locked tests ──────────────────────────────────────────

#[tokio::test]
async fn test_all_setup_endpoints_blocked_after_ready() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("oore.db");

    // Run the full setup flow to get to Ready state
    run_full_setup(&db_path).await;

    // Rebuild router from the same (now ready) database
    let store = SetupStore::connect(db_path.to_path_buf())
        .await
        .expect("failed to connect to database");
    let app = build_test_router(store, TEST_ENCRYPTION_KEY.to_vec()).await;

    // 1. Bootstrap token verify → 409
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/bootstrap-token/verify")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&json!({"token": "anything"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "already_configured");

    // 2. Configure OIDC → 409
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/oidc/configure")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer fake-token")
                .body(Body::from(
                    serde_json::to_string(&json!({
                        "issuer_url": "https://example.com",
                        "client_id": "test"
                    }))
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "already_configured");

    // 3a. Start owner OIDC → 409
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/start-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer fake-token")
                .body(Body::from(
                    serde_json::to_string(
                        &json!({"redirect_uri": "http://localhost:3000/auth/callback"}),
                    )
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "already_configured");

    // 3b. Verify owner OIDC → 409
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/owner/verify-oidc")
                .header("Content-Type", "application/json")
                .header("Authorization", "Bearer fake-token")
                .body(Body::from(
                    serde_json::to_string(&json!({"code": "test", "state": "test"})).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "already_configured");

    // 4. Complete setup → 409
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/setup/complete")
                .header("Authorization", "Bearer fake-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 409);
    let body = body_json(resp).await;
    assert_eq!(body["code"], "already_configured");

    // 5. Setup status still works and returns ready
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/public/setup-status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body = body_json(resp).await;
    assert_eq!(body["state"], "ready");
    assert_eq!(body["is_configured"], true);
}
