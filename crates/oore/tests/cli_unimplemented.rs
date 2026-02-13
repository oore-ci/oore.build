use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_oore"))
        .args(args)
        .output()
        .expect("failed to run oore binary")
}

#[test]
fn login_command_exits_with_workaround_message() {
    let output = run(&["login"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Not implemented in this release:"));
    assert!(stderr.contains("authenticate in the web UI"));
}

#[test]
fn status_command_exits_with_workaround_message() {
    let output = run(&["status"]);
    assert_eq!(output.status.code(), Some(2));

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Not implemented in this release:"));
    assert!(stderr.contains("/v1/public/setup-status"));
}

#[test]
fn config_commands_exit_with_workaround_message() {
    let set_output = run(&["config", "set", "key", "value"]);
    assert_eq!(set_output.status.code(), Some(2));
    let set_stderr = String::from_utf8_lossy(&set_output.stderr);
    assert!(set_stderr.contains("Not implemented in this release:"));
    assert!(set_stderr.contains("environment variables"));

    let get_output = run(&["config", "get", "key"]);
    assert_eq!(get_output.status.code(), Some(2));
    let get_stderr = String::from_utf8_lossy(&get_output.stderr);
    assert!(get_stderr.contains("Not implemented in this release:"));
    assert!(get_stderr.contains("daemon env vars"));
}
