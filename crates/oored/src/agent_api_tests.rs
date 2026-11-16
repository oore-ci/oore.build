//! End-to-end checks of the API features agents rely on, against the real
//! router: instance meta, the served spec, log filters, the cross-app
//! pipeline list, build wait and the error envelope.

use std::time::{Duration, Instant};

use serde_json::Value;

struct Daemon {
    base: String,
    owner: String,
    member: String,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
}

fn marker(name: &str) -> String {
    format!(
        "[oore-step] {}",
        serde_json::json!({ "event": "start", "name": name })
    )
}

async fn daemon() -> Daemon {
    let dir = tempfile::tempdir().unwrap();
    let store = crate::store::SetupStore::connect(dir.path().join("data/oore.db"))
        .await
        .unwrap();
    store.init_if_missing().await.unwrap();
    let pool = store.pool().clone();
    for statement in [
        "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
         VALUES ('owner', 'owner@example.com', 'owner', 'owner', 0, 0), \
                ('dev', 'dev@example.com', 'dev', 'developer', 0, 0)",
        "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
         VALUES ('p1', 'Kite', 'owner', 0, 0), ('p2', 'Coin', 'owner', 0, 0)",
        "INSERT INTO project_members (id, project_id, user_id, role, created_by, created_at, updated_at) \
         VALUES ('m1', 'p1', 'dev', 'viewer', 'owner', 0, 0)",
        "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
         VALUES ('pl1', 'p1', 'Release', 0, 0), ('pl2', 'p2', 'Release', 0, 0), \
                ('pl3', 'p2', 'Nightly', 0, 0)",
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
           commit_sha, queued_at, created_at, updated_at) \
         VALUES ('b1', 'p1', 'pl1', 3, 'failed', 'manual', 'abc1234def', 0, 0, 100), \
                ('b2', 'p1', 'pl1', 4, 'running', 'manual', 'fff0000aaa', 0, 0, 100)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let log = [
        marker("checkout"),
        "Cloning into kite".to_string(),
        marker("build-1"),
        "Running Gradle task".to_string(),
        "warning: something".to_string(),
        "FAILURE: Build failed".to_string(),
        "done".to_string(),
    ];
    for (sequence, line) in log.iter().enumerate() {
        sqlx::query(
            "INSERT INTO build_logs (id, build_id, sequence, content, stream, created_at) \
             VALUES (?1, 'b1', ?2, ?3, 'stdout', 0)",
        )
        .bind(format!("l{sequence}"))
        .bind(sequence as i64)
        .bind(line)
        .execute(&pool)
        .await
        .unwrap();
    }
    let sessions = crate::session::SessionStore::new(pool.clone());
    let owner = sessions.create_session("owner", 3600).await.unwrap();
    let member = sessions.create_session("dev", 3600).await.unwrap();

    let metrics = crate::observability::metrics_builder()
        .build_recorder()
        .handle();
    let router = crate::build_router(store, vec![7u8; 32], metrics)
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    Daemon {
        base,
        owner,
        member,
        pool,
        _dir: dir,
    }
}

async fn get(daemon: &Daemon, token: Option<&str>, path: &str) -> (u16, Value) {
    let mut request = reqwest::Client::new().get(format!("{}{path}", daemon.base));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    (status, response.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn meta_and_spec_are_public_and_version_needs_sign_in() {
    let daemon = daemon().await;
    let (status, meta) = get(&daemon, None, "/v1/meta").await;
    assert_eq!(status, 200, "{meta}");
    assert!(
        meta["features"]
            .as_array()
            .unwrap()
            .contains(&"diagnosis".into())
    );
    assert!(!meta["features"].as_array().unwrap().contains(&"mcp".into()));
    assert!(meta.get("version").is_none());
    assert!(meta["auth_mode"].is_string());

    sqlx::query(
        "INSERT INTO instance_preferences (id, key_storage_mode, mcp_enabled, created_at, updated_at) \
         VALUES (1, 'file', 1, 0, 0) ON CONFLICT(id) DO UPDATE SET mcp_enabled = 1",
    )
    .execute(&daemon.pool)
    .await
    .unwrap();
    let (_, meta) = get(&daemon, Some(&daemon.member), "/v1/meta").await;
    assert!(meta["features"].as_array().unwrap().contains(&"mcp".into()));
    assert!(meta["version"].is_string());

    let (status, spec) = get(&daemon, None, "/v1/openapi.json").await;
    assert_eq!(status, 200);
    assert!(spec["paths"]["/v1/builds/{build_id}/diagnosis"].is_object());
}

#[tokio::test]
async fn non_owners_cannot_turn_on_mcp() {
    let daemon = daemon().await;
    sqlx::query("UPDATE users SET role = 'admin' WHERE id = 'dev'")
        .execute(&daemon.pool)
        .await
        .unwrap();
    let client = reqwest::Client::new();
    let url = format!("{}/v1/settings/preferences", daemon.base);
    let body = serde_json::json!({ "key_storage_mode": "file", "mcp_enabled": true });
    let refused = client
        .put(&url)
        .bearer_auth(&daemon.member)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), 403);
    let error: Value = refused.json().await.unwrap();
    assert_eq!(error["code"], "mcp_owner_required");
    assert_eq!(error["retryable"], false);

    let accepted = client
        .put(&url)
        .bearer_auth(&daemon.owner)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(accepted.status(), 200);
    let preferences: Value = accepted.json().await.unwrap();
    assert_eq!(preferences["preferences"]["mcp_enabled"], true);
}

#[tokio::test]
async fn logs_filter_by_step_search_and_tail_and_speak_plain_text() {
    let daemon = daemon().await;
    let (_, body) = get(
        &daemon,
        Some(&daemon.member),
        "/v1/builds/b1/logs?step=build-1",
    )
    .await;
    assert_eq!(body["total"], 5);
    assert_eq!(body["logs"][1]["content"], "Running Gradle task");

    let (_, body) = get(
        &daemon,
        Some(&daemon.member),
        "/v1/builds/b1/logs?search=failure&context=1",
    )
    .await;
    let lines: Vec<&str> = body["logs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|chunk| chunk["content"].as_str().unwrap())
        .collect();
    assert_eq!(
        lines,
        ["warning: something", "FAILURE: Build failed", "done"]
    );

    let response = reqwest::Client::new()
        .get(format!("{}/v1/builds/b1/logs?tail=2", daemon.base))
        .bearer_auth(&daemon.member)
        .header("accept", "text/plain")
        .send()
        .await
        .unwrap();
    assert!(
        response.headers()["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/plain")
    );
    assert_eq!(
        response.text().await.unwrap(),
        "FAILURE: Build failed\ndone\n"
    );

    let (status, error) = get(&daemon, Some(&daemon.member), "/v1/builds/b1/logs?tail=0").await;
    assert_eq!(status, 400);
    assert_eq!(error["code"], "invalid_input");
    assert!(error["hint"].is_string());
}

#[tokio::test]
async fn pipelines_list_across_apps_only_shows_readable_apps() {
    let daemon = daemon().await;
    let (_, all) = get(&daemon, Some(&daemon.owner), "/v1/pipelines?name=release").await;
    assert_eq!(all["total"], 2);
    assert_eq!(all["pipelines"][0]["project_name"], "Coin");
    assert_eq!(all["pipelines"][1]["project_name"], "Kite");
    let (_, mine) = get(&daemon, Some(&daemon.member), "/v1/pipelines").await;
    assert_eq!(mine["total"], 1);
    assert_eq!(mine["pipelines"][0]["id"], "pl1");

    let (_, builds) = get(
        &daemon,
        Some(&daemon.member),
        "/v1/builds?commit_sha=ABC1234",
    )
    .await;
    assert_eq!(builds["total"], 1);
    assert_eq!(builds["builds"][0]["id"], "b1");
}

#[tokio::test]
async fn diagnosis_answers_for_a_failed_build() {
    let daemon = daemon().await;
    let (status, diagnosis) = get(
        &daemon,
        Some(&daemon.member),
        "/v1/builds/b1/diagnosis?tail=2",
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        diagnosis["log_tail"],
        serde_json::json!(["FAILURE: Build failed", "done"])
    );
    assert_eq!(diagnosis["log_tail_truncated"], true);
}

#[tokio::test]
async fn wait_answers_at_once_for_a_finished_build_and_when_the_build_changes() {
    let daemon = daemon().await;
    let started = Instant::now();
    let (status, _) = get(&daemon, Some(&daemon.member), "/v1/builds/b1?wait=30").await;
    assert_eq!(status, 200);
    assert!(started.elapsed() < Duration::from_secs(5));

    let (status, _) = get(
        &daemon,
        Some(&daemon.member),
        "/v1/builds/b2?wait=30&since_updated_at=50",
    )
    .await;
    assert_eq!(status, 200);
    assert!(started.elapsed() < Duration::from_secs(5));

    let started = Instant::now();
    let (status, _) = get(&daemon, Some(&daemon.member), "/v1/builds/b2?wait=1").await;
    assert_eq!(status, 200);
    assert!(started.elapsed() >= Duration::from_secs(1));

    let (status, error) = get(&daemon, Some(&daemon.member), "/v1/builds/b2?wait=61").await;
    assert_eq!(status, 400);
    assert_eq!(error["code"], "invalid_input");
}

#[tokio::test]
async fn invalid_bodies_name_the_rejected_field() {
    let daemon = daemon().await;
    let response = reqwest::Client::new()
        .put(format!("{}/v1/settings/preferences", daemon.base))
        .bearer_auth(&daemon.owner)
        .json(&serde_json::json!({ "mcp_enabled": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let error: Value = response.json().await.unwrap();
    assert_eq!(error["code"], "invalid_request");
    assert_eq!(error["fields"][0]["path"], "key_storage_mode");
    assert_eq!(error["retryable"], false);

    let (status, error) = get(&daemon, None, "/v1/builds/b1").await;
    assert_eq!(status, 401);
    assert_eq!(error["code"], "missing_auth");
    assert!(error["hint"].as_str().unwrap().contains("Bearer"));
}

#[tokio::test]
async fn a_retried_rerun_with_the_same_key_starts_one_build() {
    let daemon = daemon().await;
    let client = reqwest::Client::new();
    let rerun = |key: &'static str, body: &'static str| {
        client
            .post(format!("{}/v1/builds/b1/rerun", daemon.base))
            .bearer_auth(&daemon.owner)
            .header("idempotency-key", key)
            .header("content-type", "application/json")
            .body(body)
            .send()
    };
    let first = rerun("retry-1", "{}").await.unwrap();
    let first_status = first.status();
    assert!(first.headers().get("idempotent-replayed").is_none());
    let first_body: Value = first.json().await.unwrap();

    let second = rerun("retry-1", "{}").await.unwrap();
    assert_eq!(second.status(), first_status);
    assert_eq!(second.headers()["idempotent-replayed"], "true");
    let second_body: Value = second.json().await.unwrap();
    assert_eq!(first_body, second_body);

    let builds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM builds")
        .fetch_one(&daemon.pool)
        .await
        .unwrap();
    assert!(builds <= 3, "the retry started another build");

    let reused = rerun("retry-1", "{\"other\":true}").await.unwrap();
    assert_eq!(reused.status(), 409);
    let error: Value = reused.json().await.unwrap();
    assert_eq!(error["code"], "idempotency_key_reused");

    let invalid = rerun("has space", "{}").await.unwrap();
    assert_eq!(invalid.status(), 400);
}
