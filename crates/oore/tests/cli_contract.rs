//! The promises `oore` makes scripts and agents, checked on the real binary:
//! exit codes, JSON errors on stderr, extra headers, and no prompt without a
//! terminal.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;

fn oore(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let home = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_oore"));
    command
        .args(args)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home.path())
        .env("OORE_INSTALL_ROOT", home.path().join(".oore"))
        .env("OORE_CONFIG_FILE", home.path().join("config.json"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn stderr_json(output: &Output) -> serde_json::Value {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().last().unwrap_or_default();
    serde_json::from_str(line).unwrap_or_else(|_| panic!("stderr is not JSON: {stderr}"))
}

/// Answers one request with `status` and `body`, and sends back the
/// request's head.
fn one_shot_server(status: &str, body: &'static str) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    let status = status.to_string();
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut head = String::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                break;
            }
            head.push_str(&line);
        }
        let mut stream = stream;
        write!(
            stream,
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let _ = stream.flush();
        let _ = reader.read(&mut [0; 1]);
        sender.send(head).unwrap();
    });
    (url, receiver)
}

#[test]
fn a_usage_error_exits_64_and_is_json_with_json() {
    let output = oore(&["build", "lsit", "--json"], &[]);
    assert_eq!(output.status.code(), Some(64));
    assert!(output.stdout.is_empty());
    let error = stderr_json(&output);
    assert_eq!(error["code"], "usage");
    assert_eq!(error["exit_code"], 64);

    let output = oore(&["build", "lsit"], &[]);
    assert_eq!(output.status.code(), Some(64));
}

#[test]
fn help_still_exits_0() {
    let output = oore(&["--help"], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Exit codes:"));
}

#[test]
fn not_signed_in_exits_4() {
    let output = oore(&["build", "list", "--json"], &[]);
    assert_eq!(output.status.code(), Some(4));
    let error = stderr_json(&output);
    assert_eq!(error["code"], "not_signed_in");
    assert!(error["hint"].as_str().unwrap().contains("OORE_TOKEN"));
}

#[test]
fn an_api_404_exits_5_with_the_api_code_and_sends_the_extra_headers() {
    let (url, head) = one_shot_server(
        "404 Not Found",
        r#"{"error":"Build not found","code":"build_not_found"}"#,
    );
    let output = oore(
        &[
            "build",
            "show",
            "b_missing",
            "--json",
            "--header",
            "X-Flag: two",
        ],
        &[
            ("OORE_URL", &url),
            ("OORE_TOKEN", "oore_test"),
            ("OORE_HTTP_HEADERS", "CF-Access-Client-Id: abc\nX-Flag: one"),
        ],
    );
    assert_eq!(output.status.code(), Some(5), "{output:?}");
    assert!(output.stdout.is_empty());
    let error = stderr_json(&output);
    assert_eq!(error["code"], "build_not_found");
    assert_eq!(error["status"], 404);

    let head = head.recv().unwrap().to_ascii_lowercase();
    assert!(head.starts_with("get /v1/builds/b_missing "), "{head}");
    assert!(head.contains("authorization: bearer oore_test"));
    assert!(head.contains("cf-access-client-id: abc"));
    // --header wins over the environment.
    assert!(head.contains("x-flag: two"));
    assert!(!head.contains("x-flag: one"));
}

#[test]
fn the_old_variable_names_still_work() {
    let (url, head) = one_shot_server("403 Forbidden", r#"{"error":"No","code":"forbidden"}"#);
    let output = oore(
        &["build", "show", "b1"],
        &[("OORE_DAEMON_URL", &url), ("OORE_SESSION_TOKEN", "old")],
    );
    assert_eq!(output.status.code(), Some(4));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Error: build lookup failed (HTTP 403)")
    );
    assert!(
        head.recv()
            .unwrap()
            .to_ascii_lowercase()
            .contains("bearer old")
    );
}

#[test]
fn install_without_a_terminal_names_the_flag() {
    let output = oore(&["install"], &[]);
    assert_ne!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--profile"));
}

#[test]
fn restore_without_a_terminal_needs_yes() {
    let output = oore(
        &[
            "backup",
            "restore",
            "--input",
            "/nonexistent.tar.gz",
            "--json",
        ],
        &[],
    );
    assert_eq!(output.status.code(), Some(64));
    let error = stderr_json(&output);
    assert!(error["error"].as_str().unwrap().contains("--yes"));
}

#[test]
fn the_command_tree_is_json() {
    let output = oore(&["commands", "--json"], &[]);
    assert_eq!(output.status.code(), Some(0));
    let tree: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(tree["exit_codes"]["64"], "usage error");
    let build = tree["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|command| command["name"] == "build")
        .unwrap();
    let names: Vec<&str> = build["subcommands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|command| command["name"].as_str().unwrap())
        .collect();
    for wanted in [
        "run", "watch", "list", "show", "logs", "why", "cancel", "rerun",
    ] {
        assert!(names.contains(&wanted), "{wanted} missing from {names:?}");
    }
}

#[test]
fn completions_print_a_script() {
    let output = oore(&["completions", "bash"], &[]);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("_oore"));
}

#[test]
fn version_is_json_with_json() {
    let output = oore(&["version", "--json"], &[]);
    assert_eq!(output.status.code(), Some(0));
    let version: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(version["binary"], env!("CARGO_PKG_VERSION"));
}
