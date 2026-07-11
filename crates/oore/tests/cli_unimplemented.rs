use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_oore"))
        .args(args)
        .output()
        .expect("failed to run oore binary")
}

fn run_with_env(args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_oore"));
    command.args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("failed to run oore binary")
}

fn temp_config_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "oore-cli-test-{}-{nanos}-{}",
        std::process::id(),
        name
    ));
    fs::create_dir_all(&dir).expect("create temp config dir");
    dir.join("config.json")
}

fn cleanup_config_path(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

fn http_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Unknown",
    }
}

fn spawn_stub_server<F>(expected_requests: usize, handler: F) -> (String, thread::JoinHandle<()>)
where
    F: Fn(&str, &str, &str) -> (u16, String) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("server local addr");

    let handle = thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut buf = [0u8; 16 * 1024];
            let n = stream.read(&mut buf).expect("read request bytes");
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let first_line = req.lines().next().unwrap_or("");
            let mut parts = first_line.split_whitespace();
            let method = parts.next().unwrap_or("");
            let path = parts.next().unwrap_or("/");
            let (status, body) = handler(method, path, &req);

            let response = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                http_reason(status),
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            stream.flush().expect("flush response");
        }
    });

    (format!("http://{}", addr), handle)
}

#[test]
fn login_command_rejects_invalid_token() {
    let config_path = temp_config_path("login-invalid-token");
    let cfg = config_path.to_string_lossy().into_owned();
    let (daemon_url, server) = spawn_stub_server(1, |_, path, _| {
        assert_eq!(path, "/v1/users/me");
        (
            401,
            r#"{"error":"unauthorized","code":"unauthorized","details":"token invalid"}"#
                .to_string(),
        )
    });

    let output = run_with_env(
        &[
            "login",
            "--daemon-url",
            daemon_url.as_str(),
            "--token",
            "invalid",
        ],
        &[("OORE_CONFIG_FILE", cfg.as_str())],
    );
    server.join().expect("server thread");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Token was rejected"));

    cleanup_config_path(&config_path);
}

#[test]
fn login_command_local_mode_stores_session_token() {
    let config_path = temp_config_path("login-local-success");
    let cfg = config_path.to_string_lossy().into_owned();
    let (daemon_url, server) = spawn_stub_server(1, |method, path, _| {
        assert_eq!(method, "POST");
        assert_eq!(path, "/v1/auth/local/login");
        (
            200,
            r#"{
              "session_token":"session-local-123",
              "expires_at":1893456000,
              "user":{"email":"owner@local","oidc_subject":"local-owner","user_id":"u-1","role":"owner"}
            }"#
            .to_string(),
        )
    });

    let output = run_with_env(
        &["login", "--daemon-url", daemon_url.as_str()],
        &[("OORE_CONFIG_FILE", cfg.as_str())],
    );
    server.join().expect("server thread");

    assert_eq!(output.status.code(), Some(0));
    let config_raw = fs::read_to_string(&config_path).expect("read saved config");
    assert!(config_raw.contains("\"session_token\": \"session-local-123\""));
    assert!(config_raw.contains(daemon_url.as_str()));

    cleanup_config_path(&config_path);
}

#[test]
fn login_command_surfaces_loopback_rejection() {
    let config_path = temp_config_path("login-loopback-rejected");
    let cfg = config_path.to_string_lossy().into_owned();
    let (daemon_url, server) = spawn_stub_server(1, |method, path, _| {
        assert_eq!(method, "POST");
        assert_eq!(path, "/v1/auth/local/login");
        (
            403,
            r#"{
              "error":"local login is allowed only from loopback clients",
              "code":"local_login_loopback_required",
              "details":"client ip is not loopback"
            }"#
            .to_string(),
        )
    });

    let output = run_with_env(
        &["login", "--daemon-url", daemon_url.as_str()],
        &[("OORE_CONFIG_FILE", cfg.as_str())],
    );
    server.join().expect("server thread");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("local_login_loopback_required"));

    cleanup_config_path(&config_path);
}

#[test]
fn doctor_accepts_repeatable_platforms_and_reports_json_statuses() {
    let output = run(&[
        "doctor",
        "--platform",
        "android",
        "--platform",
        "ios",
        "--json",
    ]);
    assert!(matches!(output.status.code(), Some(0 | 1)));

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor should return JSON");
    let checks = report["checks"].as_array().expect("checks array");
    assert!(checks.iter().any(|check| check["name"] == "java"));
    assert!(checks.iter().any(|check| check["name"] == "android_sdk"));
    assert!(checks.iter().any(|check| check["name"] == "xcode"));
    assert!(checks.iter().all(|check| matches!(
        check["status"].as_str(),
        Some("ok" | "warning" | "missing" | "skipped")
    )));
}

#[test]
fn doctor_all_includes_each_platform() {
    let output = run(&["doctor", "--all", "--json"]);
    assert!(matches!(output.status.code(), Some(0 | 1)));

    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("doctor should return JSON");
    let checks = report["checks"].as_array().expect("checks array");
    assert!(checks.iter().any(|check| check["name"] == "java"));
    assert!(checks.iter().any(|check| check["name"] == "android_sdk"));
    assert!(checks.iter().any(|check| check["name"] == "xcode"));
}

#[test]
fn config_set_get_round_trip_supported_keys() {
    let config_path = temp_config_path("config-roundtrip");
    let cfg = config_path.to_string_lossy().into_owned();

    let set_daemon = run_with_env(
        &["config", "set", "daemon_url", "http://127.0.0.1:9000"],
        &[("OORE_CONFIG_FILE", cfg.as_str())],
    );
    assert_eq!(set_daemon.status.code(), Some(0));

    let set_token = run_with_env(
        &["config", "set", "session_token", "token-abc"],
        &[("OORE_CONFIG_FILE", cfg.as_str())],
    );
    assert_eq!(set_token.status.code(), Some(0));

    let get_daemon = run_with_env(
        &["config", "get", "daemon_url"],
        &[("OORE_CONFIG_FILE", cfg.as_str())],
    );
    assert_eq!(get_daemon.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&get_daemon.stdout).trim(),
        "http://127.0.0.1:9000"
    );

    let get_token = run_with_env(
        &["config", "get", "session_token"],
        &[("OORE_CONFIG_FILE", cfg.as_str())],
    );
    assert_eq!(get_token.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&get_token.stdout).trim(),
        "token-abc"
    );

    cleanup_config_path(&config_path);
}

#[test]
fn config_commands_reject_unsupported_key_with_exit_code_2() {
    let output = run(&["config", "set", "unknown_key", "value"]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unsupported config key"));
}

#[test]
fn status_command_errors_when_daemon_unreachable() {
    let output = run(&["status", "--daemon-url", "http://127.0.0.1:1"]);
    assert_eq!(output.status.code(), Some(1));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("failed to reach daemon"));
    assert!(stderr.contains("/v1/public/setup-status"));
}

#[test]
fn status_without_token_prints_setup_summary_only() {
    let (daemon_url, server) = spawn_stub_server(1, |method, path, _| {
        assert_eq!(method, "GET");
        assert_eq!(path, "/v1/public/setup-status");
        (
            200,
            r#"{
              "instance_id":"inst-1",
              "state":"ready",
              "runtime_mode":"local",
              "remote_auth_mode":"oidc",
              "setup_mode":false,
              "is_configured":true
            }"#
            .to_string(),
        )
    });

    let output = run(&["status", "--daemon-url", daemon_url.as_str()]);
    server.join().expect("server thread");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Instance: inst-1"));
    assert!(stdout.contains("Authenticated: no"));
}

#[test]
fn status_with_valid_token_prints_queue_build_and_runner_details() {
    let (daemon_url, server) = spawn_stub_server(6, |method, path, req| {
        assert_eq!(method, "GET");
        if path == "/v1/public/setup-status" {
            return (
                200,
                r#"{
                  "instance_id":"inst-2",
                  "state":"ready",
                  "runtime_mode":"local",
                  "remote_auth_mode":"oidc",
                  "setup_mode":false,
                  "is_configured":true
                }"#
                .to_string(),
            );
        }
        if path == "/v1/users/me" {
            let req_lower = req.to_ascii_lowercase();
            assert!(req_lower.contains("authorization: bearer token-ok"));
            return (
                200,
                r#"{
                  "user":{
                    "id":"u1",
                    "email":"owner@local",
                    "display_name":"Owner",
                    "role":"owner",
                    "status":"active",
                    "created_at":1,
                    "updated_at":1
                  }
                }"#
                .to_string(),
            );
        }
        if path.starts_with("/v1/builds?status=queued") {
            return (200, r#"{"builds":[],"total":2}"#.to_string());
        }
        if path.starts_with("/v1/builds?status=running") {
            return (200, r#"{"builds":[],"total":1}"#.to_string());
        }
        if path.starts_with("/v1/builds?limit=5") || path.starts_with("/v1/builds?offset=0&limit=5")
        {
            return (
                200,
                r#"{
                  "builds":[
                    {
                      "id":"b-1","project_id":"p-1","pipeline_id":"pl-1","build_number":42,
                      "trigger_type":"manual","trigger_ref":"cli","trigger_event":"manual",
                      "branch":"main","commit_sha":"abc1234","status":"succeeded",
                      "config_snapshot":{"repo_url":"https://example.com/repo.git"},
                      "runner_id":"r-1","queued_at":1,"started_at":2,"finished_at":3,
                      "created_at":1,"updated_at":3
                    }
                  ],
                  "total":1
                }"#
                .to_string(),
            );
        }
        if path == "/v1/runners" {
            return (
                200,
                r#"{
                  "runners":[
                    {"id":"r-1","name":"runner-a","status":"online","capabilities":{},"created_at":1,"updated_at":1},
                    {"id":"r-2","name":"runner-b","status":"offline","capabilities":{},"created_at":1,"updated_at":1}
                  ]
                }"#
                .to_string(),
            );
        }

        panic!("unexpected path: {path}");
    });

    let output = run(&[
        "status",
        "--daemon-url",
        daemon_url.as_str(),
        "--token",
        "token-ok",
    ]);
    server.join().expect("server thread");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Authenticated: yes"));
    assert!(stdout.contains("Queue depth:   2"));
    assert!(stdout.contains("Active builds: 3"));
    assert!(stdout.contains("Runners:       2 total (1 online/busy)"));
    assert!(stdout.contains("Recent builds:"));
    assert!(stdout.contains("#42 succeeded (manual)"));
}
