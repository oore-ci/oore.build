//! End-to-end tests of `/mcp` on a real daemon router.

use serde_json::{Value, json};

struct Daemon {
    base: String,
    pool: sqlx::SqlitePool,
    dev: String,
    qa: String,
    outsider: String,
    _dir: tempfile::TempDir,
}

async fn daemon() -> Daemon {
    let dir = tempfile::tempdir().unwrap();
    let store = crate::store::SetupStore::connect(dir.path().join("data/oore.db"))
        .await
        .unwrap();
    let pool = store.pool().clone();
    for statement in [
        "INSERT INTO users (id, email, oidc_subject, display_name, role, created_at, updated_at) \
         VALUES ('dev', 'dev@example.com', 'dev', NULL, 'developer', 0, 0), \
                ('qa', 'qa@example.com', 'qa', NULL, 'qa_viewer', 0, 0), \
                ('out', 'out@example.com', 'out', NULL, 'developer', 0, 0)",
        "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
         VALUES ('p1', 'Kite', 'dev', 0, 0), ('p2', 'Secret', 'out', 0, 0)",
        "INSERT INTO project_members (id, project_id, user_id, role, created_by, created_at, updated_at) \
         VALUES ('m1', 'p1', 'dev', 'developer', 'dev', 0, 0), \
                ('m2', 'p1', 'qa', 'viewer', 'dev', 0, 0), \
                ('m3', 'p2', 'out', 'developer', 'out', 0, 0)",
        "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
         VALUES ('pl1', 'p1', 'Android', 0, 0), ('pl2', 'p2', 'Android', 0, 0)",
        "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
           queued_at, created_at, updated_at, notified_status, config_snapshot, step_results) \
         VALUES ('b1', 'p1', 'pl1', 12, 'failed', 'manual', 0, 0, 0, 'failed', '{\"big\":\"snapshot\"}', \
                 '[{\"name\":\"Install\",\"status\":\"succeeded\",\"started_at\":0,\"finished_at\":1,\"duration_ms\":1},\
                   {\"name\":\"Build Android\",\"status\":\"failed\",\"exit_code\":1,\"started_at\":1,\"finished_at\":2,\"duration_ms\":1}]'), \
                ('b2', 'p2', 'pl2', 1, 'succeeded', 'manual', 0, 0, 0, 'succeeded', '{}', NULL)",
        "INSERT INTO build_logs (id, build_id, sequence, stream, content, created_at) VALUES \
           ('l0', 'b1', 0, 'stdout', '[oore-step] {\"event\":\"start\",\"name\":\"Install\",\"command\":\"x\"}', 0), \
           ('l1', 'b1', 1, 'stdout', 'fetching packages', 0), \
           ('l2', 'b1', 2, 'stdout', '[oore-step] {\"event\":\"start\",\"name\":\"Build Android\",\"command\":\"x\"}', 0), \
           ('l3', 'b1', 3, 'stdout', 'FAILURE: Build failed with an exception.', 0), \
           ('l4', 'b1', 4, 'stdout', '* What went wrong:', 0), \
           ('l5', 'b1', 5, 'stdout', 'Execution failed for task :app:mergeReleaseResources.', 0)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let sessions = crate::session::SessionStore::new(pool.clone());
    let dev = sessions.create_session("dev", 3600).await.unwrap();
    let qa = sessions.create_session("qa", 3600).await.unwrap();
    let outsider = sessions.create_session("out", 3600).await.unwrap();

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
        pool,
        dev,
        qa,
        outsider,
        _dir: dir,
    }
}

/// Turns MCP on. Adds the preference column when this database does not
/// have it yet.
async fn enable(pool: &sqlx::SqlitePool) {
    let _ = sqlx::query(
        "ALTER TABLE instance_preferences ADD COLUMN mcp_enabled INTEGER NOT NULL DEFAULT 0",
    )
    .execute(pool)
    .await;
    sqlx::query(
        "INSERT INTO instance_preferences (id, created_at, updated_at, mcp_enabled) VALUES (1, 0, 0, 1) \
         ON CONFLICT(id) DO UPDATE SET mcp_enabled = 1",
    )
    .execute(pool)
    .await
    .unwrap();
}

const VERSION: &str = "2026-07-28";

fn rpc(method: &str, params: Value) -> Value {
    let mut params = params;
    params["_meta"] = json!({
        "io.modelcontextprotocol/protocolVersion": VERSION,
        "io.modelcontextprotocol/clientInfo": {"name": "test", "version": "1"},
        "io.modelcontextprotocol/clientCapabilities": {},
    });
    json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
}

async fn post(d: &Daemon, token: Option<&str>, body: &Value) -> reqwest::Response {
    let method = body["method"].as_str().unwrap_or_default().to_string();
    let mut request = reqwest::Client::new()
        .post(format!("{}/mcp", d.base))
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", VERSION)
        .header("mcp-method", method)
        .json(body);
    if let Some(name) = body["params"]["name"].as_str() {
        request = request.header("mcp-name", name);
    }
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    request.send().await.unwrap()
}

/// The JSON-RPC answer, from a JSON or an event-stream body.
async fn answer(response: reqwest::Response) -> Value {
    let status = response.status();
    let text = response.text().await.unwrap();
    assert!(status.is_success(), "{status}: {text}");
    if let Ok(value) = serde_json::from_str(&text) {
        return value;
    }
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .filter_map(|data| serde_json::from_str::<Value>(data.trim()).ok())
        .find(|value| value.get("id").is_some())
        .unwrap_or_else(|| panic!("no JSON-RPC answer in {text}"))
}

async fn call(d: &Daemon, token: &str, tool: &str, arguments: Value) -> Value {
    let body = rpc("tools/call", json!({"name": tool, "arguments": arguments}));
    let reply = answer(post(d, Some(token), &body).await).await;
    reply
        .get("result")
        .cloned()
        .unwrap_or_else(|| panic!("no result: {reply}"))
}

fn tool_names(reply: &Value) -> Vec<String> {
    reply["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn mcp_is_hidden_until_an_owner_turns_it_on() {
    let d = daemon().await;
    let body = rpc("tools/list", json!({}));
    assert_eq!(post(&d, Some(&d.dev), &body).await.status(), 404);

    enable(&d.pool).await;
    let anonymous = post(&d, None, &body).await;
    assert_eq!(anonymous.status(), 401);
    assert!(anonymous.headers().contains_key("www-authenticate"));
    let bad = post(&d, Some("not-a-token"), &body).await;
    assert_eq!(bad.status(), 401);
    assert_eq!(post(&d, Some(&d.dev), &body).await.status(), 200);
}

#[tokio::test]
async fn browsers_from_another_origin_are_refused() {
    let d = daemon().await;
    enable(&d.pool).await;
    let body = rpc("tools/list", json!({}));
    let send = |origin: String| {
        reqwest::Client::new()
            .post(format!("{}/mcp", d.base))
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", VERSION)
            .header("mcp-method", "tools/list")
            .header("origin", origin)
            .bearer_auth(&d.dev)
            .json(&body)
            .send()
    };
    assert_eq!(
        send("https://evil.example".into()).await.unwrap().status(),
        403
    );
    let same = d.base.clone();
    assert_eq!(send(same).await.unwrap().status(), 200);
}

#[tokio::test]
async fn older_clients_can_initialize() {
    let d = daemon().await;
    enable(&d.pool).await;
    let response = reqwest::Client::new()
        .post(format!("{}/mcp", d.base))
        .header("accept", "application/json, text/event-stream")
        .bearer_auth(&d.dev)
        .json(&json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-11-25", "capabilities": {},
                       "clientInfo": {"name": "legacy", "version": "1"}}
        }))
        .send()
        .await
        .unwrap();
    let reply = answer(response).await;
    assert_eq!(reply["result"]["serverInfo"]["name"], "oore");
    assert!(reply["result"]["capabilities"]["tools"].is_object());

    let list = reqwest::Client::new()
        .post(format!("{}/mcp", d.base))
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-11-25")
        .bearer_auth(&d.dev)
        .json(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}))
        .send()
        .await
        .unwrap();
    assert!(tool_names(&answer(list).await).contains(&"diagnose_build".to_string()));
}

#[tokio::test]
async fn tools_are_listed_by_what_the_caller_may_do() {
    let d = daemon().await;
    enable(&d.pool).await;
    let body = rpc("tools/list", json!({}));
    let dev = tool_names(&answer(post(&d, Some(&d.dev), &body).await).await);
    for name in [
        "list_apps",
        "get_build_logs",
        "diagnose_build",
        "run_build",
        "wait_for_build",
    ] {
        assert!(
            dev.contains(&name.to_string()),
            "{name} missing from {dev:?}"
        );
    }
    let qa = tool_names(&answer(post(&d, Some(&d.qa), &body).await).await);
    assert!(qa.contains(&"get_build".to_string()));
    assert!(!qa.contains(&"run_build".to_string()));
    assert!(!qa.contains(&"cancel_build".to_string()));

    let listed = answer(post(&d, Some(&d.dev), &body).await).await;
    let get_build = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "get_build")
        .unwrap()
        .clone();
    assert_eq!(get_build["annotations"]["readOnlyHint"], true);
    let cancel = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "cancel_build")
        .unwrap()
        .clone();
    assert_eq!(cancel["annotations"]["destructiveHint"], true);
}

#[tokio::test]
async fn read_tools_answer_small_and_respect_access() {
    let d = daemon().await;
    enable(&d.pool).await;

    let apps = call(&d, &d.dev, "list_apps", json!({})).await;
    let names: Vec<&str> = apps["structuredContent"]["apps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|app| app["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Kite"]);

    // Apps and pipelines can be named instead of using IDs.
    let builds = call(
        &d,
        &d.dev,
        "list_builds",
        json!({"app": "kite", "pipeline": "Android"}),
    )
    .await;
    let builds = builds["structuredContent"]["builds"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(builds.len(), 1);
    assert_eq!(builds[0]["failed_step"]["name"], "Build Android");

    let build = call(&d, &d.dev, "get_build", json!({"build_id": "b1"})).await;
    assert_eq!(build["isError"], false);
    assert!(build["structuredContent"].get("config_snapshot").is_none());
    assert_eq!(
        build["structuredContent"]["steps"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Another app's build is the API's 404, not a leak.
    let other = call(&d, &d.dev, "get_build", json!({"build_id": "b2"})).await;
    assert_eq!(other["isError"], true);
    let error: Value = serde_json::from_str(other["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(error["status"], 404);
    let other = call(&d, &d.outsider, "get_build", json!({"build_id": "b1"})).await;
    assert_eq!(other["isError"], true);

    let logs = call(
        &d,
        &d.dev,
        "get_build_logs",
        json!({"build_id": "b1", "step": "Build Android", "search": "wrong"}),
    )
    .await;
    assert_eq!(
        logs["structuredContent"]["lines"],
        json!(["4 * What went wrong:"])
    );
    assert_eq!(
        logs["structuredContent"]["steps"],
        json!(["Install", "Build Android"])
    );

    let diagnosis = call(&d, &d.dev, "diagnose_build", json!({"build_id": "b1"})).await;
    let diagnosis = &diagnosis["structuredContent"];
    assert_eq!(diagnosis["failed_step"]["name"], "Build Android");
    assert_eq!(diagnosis["failed_step"]["exit_code"], 1);
    let tail = diagnosis["log_tail"].as_array().unwrap();
    assert_eq!(tail.first().unwrap(), "== step started: Build Android");
    assert!(diagnosis["hint"].is_object(), "{diagnosis}");
}

#[tokio::test]
async fn build_tools_go_through_the_api_checks() {
    let d = daemon().await;
    enable(&d.pool).await;

    // A finished build answers at once.
    let waited = call(&d, &d.dev, "wait_for_build", json!({"build_id": "b1"})).await;
    assert_eq!(waited["structuredContent"]["done"], true);
    assert_eq!(waited["structuredContent"]["status"], "failed");

    // A QA viewer is not offered build tools, and calling one anyway fails.
    let body = rpc(
        "tools/call",
        json!({"name": "cancel_build", "arguments": {"build_id": "b1"}}),
    );
    let reply = answer(post(&d, Some(&d.qa), &body).await).await;
    assert!(reply.get("error").is_some(), "{reply}");

    // Bad arguments are the tool's error, with a code.
    let missing = call(&d, &d.dev, "get_build", json!({})).await;
    assert_eq!(missing["isError"], true);
    assert!(
        missing["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("invalid_arguments")
    );
}
