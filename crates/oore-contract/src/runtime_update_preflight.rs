//! Preflight checks for the one-click backend update.
//!
//! `oored` runs these before it queues an update and serves them to the web
//! UI; `oore doctor --update` runs the same checks on the backend Mac. Each
//! check names what failed and how to fix it instead of collapsing the
//! managed-service fingerprint into one boolean.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::{RuntimeUpdateCheck, RuntimeUpdateCheckStatus, RuntimeUpdatePreflight};

pub const DAEMON_SERVICE_LABEL: &str = "build.oore.oored";
pub const UPDATE_SERVICE_LABEL: &str = "build.oore.oore-updater";
pub const DAEMON_SERVICE_PLIST: &str = "/Library/LaunchDaemons/build.oore.oored.plist";
pub const UPDATE_SERVICE_PLIST: &str = "/Library/LaunchDaemons/build.oore.oore-updater.plist";
pub const RUNNER_SERVICE_PLIST: &str = "/Library/LaunchDaemons/build.oore.oore-runner.plist";
pub const UPDATE_REQUEST_DIR: &str = "run/runtime-update-queue";
pub const UPDATE_REQUEST_FILE: &str = "request.json";
pub const SERVICE_PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

const REPAIR_FIX: &str = "Run `oore setup` from Terminal on the backend Mac as the account that owns the install to repair the managed services. No reinstall is needed.";

/// Runs every check for the install rooted at `install_root`, as the account
/// that runs this process. That account must be the one the managed services
/// run as.
pub fn run(install_root: &Path) -> RuntimeUpdatePreflight {
    let checks = if cfg!(target_os = "macos") {
        macos_checks(install_root)
    } else {
        vec![fail(
            "platform",
            "macOS host",
            format!(
                "This backend runs on {}; one-click updates need the managed macOS launchd services.",
                std::env::consts::OS
            ),
            "Update this backend with your platform's own service tooling.",
        )]
    };
    preflight(checks)
}

fn preflight(checks: Vec<RuntimeUpdateCheck>) -> RuntimeUpdatePreflight {
    RuntimeUpdatePreflight {
        ready: checks
            .iter()
            .all(|check| check.status != RuntimeUpdateCheckStatus::Fail),
        checked_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64,
        checks,
    }
}

/// The first failing check, for API errors that must name one reason.
pub fn first_failure(preflight: &RuntimeUpdatePreflight) -> Option<&RuntimeUpdateCheck> {
    preflight
        .checks
        .iter()
        .find(|check| check.status == RuntimeUpdateCheckStatus::Fail)
}

fn macos_checks(install_root: &Path) -> Vec<RuntimeUpdateCheck> {
    let mut checks = Vec::new();

    let updater = install_root.join("bin/oore");
    checks.push(if updater.is_file() {
        pass(
            "installed_updater",
            "Installed updater",
            updater.display().to_string(),
        )
    } else {
        fail(
            "installed_updater",
            "Installed updater",
            format!("{} was not found beside oored.", updater.display()),
            "Reinstall Oore with the installer from Terminal on the backend Mac.",
        )
    });

    checks.push(if Path::new(DAEMON_SERVICE_PLIST).is_file() {
        pass(
            "daemon_service",
            "Backend launchd service",
            DAEMON_SERVICE_PLIST.to_string(),
        )
    } else {
        fail(
            "daemon_service",
            "Backend launchd service",
            format!("{DAEMON_SERVICE_PLIST} is missing, so oored is not a boot-time service."),
            REPAIR_FIX,
        )
    });

    let account = current_user_name()
        .ok_or("Could not resolve the account name with `id -un`.")
        .and_then(|user| {
            std::env::var_os("HOME")
                .map(|home| (user, PathBuf::from(home)))
                .ok_or("HOME is not set for this process.")
        });
    let (service_user, service_home) = match account {
        Ok(account) => {
            checks.push(pass(
                "service_account",
                "Service account",
                format!("{} ({})", account.0, account.1.display()),
            ));
            account
        }
        Err(detail) => {
            checks.push(fail(
                "service_account",
                "Service account",
                detail.to_string(),
                REPAIR_FIX,
            ));
            return checks;
        }
    };

    checks.push(
        match update_supervisor_definition_problem(
            install_root,
            Path::new(UPDATE_SERVICE_PLIST),
            &service_user,
            &service_home,
        ) {
            None => pass(
                "updater_definition",
                "Update supervisor definition",
                UPDATE_SERVICE_PLIST.to_string(),
            ),
            Some(problem) => fail(
                "updater_definition",
                "Update supervisor definition",
                problem,
                REPAIR_FIX,
            ),
        },
    );

    checks.push(
        match update_supervisor_loaded_problem(install_root, &service_user, &service_home) {
            None => pass(
                "updater_loaded",
                "Update supervisor loaded",
                format!("system/{UPDATE_SERVICE_LABEL}"),
            ),
            Some(problem) => fail(
                "updater_loaded",
                "Update supervisor loaded",
                problem,
                &format!(
                    "Run `sudo launchctl bootout system/{UPDATE_SERVICE_LABEL}; sudo launchctl bootstrap system {UPDATE_SERVICE_PLIST}` on the backend Mac, or `oore setup` from Terminal."
                ),
            ),
        },
    );

    checks.push(
        match backend_profile_from_manifest(&install_root.join("install-manifest.json")).and_then(
            |profile| runner_service_problem(profile.as_deref(), Path::new(RUNNER_SERVICE_PLIST)),
        ) {
            Ok(detail) => pass("runner_service", "Runner service", detail),
            Err(problem) => fail("runner_service", "Runner service", problem, REPAIR_FIX),
        },
    );

    checks.push(recovery_privileges_check(&service_user));
    checks
}

fn pass(id: &str, label: &str, detail: String) -> RuntimeUpdateCheck {
    RuntimeUpdateCheck {
        id: id.to_string(),
        label: label.to_string(),
        status: RuntimeUpdateCheckStatus::Pass,
        detail: Some(detail),
        fix: None,
    }
}

fn fail(id: &str, label: &str, detail: String, fix: &str) -> RuntimeUpdateCheck {
    RuntimeUpdateCheck {
        id: id.to_string(),
        label: label.to_string(),
        status: RuntimeUpdateCheckStatus::Fail,
        detail: Some(detail),
        fix: Some(fix.to_string()),
    }
}

fn current_user_name() -> Option<String> {
    let output = Command::new("/usr/bin/id").arg("-un").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let name = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!name.is_empty()).then_some(name)
}

fn command_diagnostic(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("exited with {}", output.status)
    } else {
        stderr.chars().take(500).collect()
    }
}

fn update_supervisor_loaded_problem(
    install_root: &Path,
    service_user: &str,
    service_home: &Path,
) -> Option<String> {
    let service = format!("system/{UPDATE_SERVICE_LABEL}");
    let output = match Command::new("/bin/launchctl")
        .args(["print", &service])
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return Some(format!(
                "Could not run `launchctl print {service}`: {error}"
            ));
        }
    };
    if !output.status.success() {
        return Some(format!(
            "launchd has no {service} job: {}",
            command_diagnostic(&output)
        ));
    }
    match String::from_utf8(output.stdout) {
        Ok(loaded) => {
            loaded_update_supervisor_problem(&loaded, install_root, service_user, service_home).map(
                |problem| format!("The loaded job differs from the managed definition: {problem}"),
            )
        }
        Err(_) => Some(format!(
            "`launchctl print {service}` returned invalid UTF-8."
        )),
    }
}

fn update_supervisor_definition_problem(
    install_root: &Path,
    path: &Path,
    service_user: &str,
    service_home: &Path,
) -> Option<String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Some(format!("{} is missing.", path.display()));
        }
        Err(error) => return Some(format!("Could not inspect {}: {error}", path.display())),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Some(format!("{} is not a regular file.", path.display()));
    }
    if metadata.uid() != 0 || metadata.gid() != 0 {
        return Some(format!(
            "{} is owned by uid {} gid {}; it must be owned by root:wheel.",
            path.display(),
            metadata.uid(),
            metadata.gid()
        ));
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode != 0o644 {
        return Some(format!(
            "{} has mode {mode:04o}; it must be 0644.",
            path.display()
        ));
    }
    let output = match Command::new("/usr/bin/plutil")
        .args(["-convert", "json", "-o", "-"])
        .arg(path)
        .output()
    {
        Ok(output) => output,
        Err(error) => return Some(format!("Could not run plutil: {error}")),
    };
    if !output.status.success() {
        return Some(format!(
            "plutil could not read {}: {}",
            path.display(),
            command_diagnostic(&output)
        ));
    }
    match serde_json::from_slice::<Value>(&output.stdout) {
        Ok(document) => {
            update_supervisor_document_problem(&document, install_root, service_user, service_home)
        }
        Err(error) => Some(format!("plutil returned invalid JSON: {error}")),
    }
}

fn expect_str(
    document: &serde_json::Map<String, Value>,
    key: &str,
    expected: &str,
) -> Option<String> {
    match document.get(key).and_then(Value::as_str) {
        Some(value) if value == expected => None,
        Some(value) => Some(format!("{key} is `{value}`, expected `{expected}`.")),
        None => Some(format!(
            "{key} is missing or not a string, expected `{expected}`."
        )),
    }
}

fn expect_bool(
    document: &serde_json::Map<String, Value>,
    key: &str,
    expected: bool,
) -> Option<String> {
    (document.get(key).and_then(Value::as_bool) != Some(expected))
        .then(|| format!("{key} must be {expected}."))
}

fn update_supervisor_document_problem(
    document: &Value,
    install_root: &Path,
    service_user: &str,
    service_home: &Path,
) -> Option<String> {
    let Some(document) = document.as_object() else {
        return Some("The definition is not a dictionary.".to_string());
    };
    let managed_keys = [
        "Label",
        "UserName",
        "ProgramArguments",
        "WorkingDirectory",
        "EnvironmentVariables",
        "Umask",
        "RunAtLoad",
        "KeepAlive",
        "StandardOutPath",
        "StandardErrorPath",
    ];
    if let Some(key) = managed_keys
        .iter()
        .find(|key| !document.contains_key(**key))
    {
        return Some(format!("{key} is missing."));
    }
    let mut unexpected = document
        .keys()
        .filter(|key| !managed_keys.contains(&key.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    if !unexpected.is_empty() {
        unexpected.sort_unstable();
        return Some(format!("Unexpected keys: {}.", unexpected.join(", ")));
    }
    let root = install_root.to_string_lossy();
    let expected_log = install_root.join("logs/update-supervisor.log");
    let expected_log = expected_log.to_string_lossy();
    if let Some(problem) = expect_str(document, "Label", UPDATE_SERVICE_LABEL)
        .or_else(|| expect_str(document, "UserName", service_user))
        .or_else(|| expect_str(document, "WorkingDirectory", &root))
        .or_else(|| {
            (document.get("Umask").and_then(Value::as_u64) != Some(0o77))
                .then(|| "Umask must be 63 (0077).".to_string())
        })
        .or_else(|| expect_bool(document, "RunAtLoad", false))
        .or_else(|| expect_bool(document, "KeepAlive", false))
        .or_else(|| expect_str(document, "StandardOutPath", &expected_log))
        .or_else(|| expect_str(document, "StandardErrorPath", &expected_log))
    {
        return Some(problem);
    }
    let Some(environment) = document
        .get("EnvironmentVariables")
        .and_then(Value::as_object)
    else {
        return Some("EnvironmentVariables is not a dictionary.".to_string());
    };
    let home = service_home.to_string_lossy();
    if let Some(problem) = expect_str(environment, "HOME", &home)
        .or_else(|| expect_str(environment, "PATH", SERVICE_PATH))
        .or_else(|| expect_str(environment, "OORE_INSTALL_ROOT", &root))
    {
        return Some(format!("EnvironmentVariables: {problem}"));
    }
    if environment.len() != 3 {
        let mut extra = environment
            .keys()
            .filter(|key| !["HOME", "PATH", "OORE_INSTALL_ROOT"].contains(&key.as_str()))
            .map(String::as_str)
            .collect::<Vec<_>>();
        extra.sort_unstable();
        return Some(format!(
            "EnvironmentVariables has unexpected keys: {}.",
            extra.join(", ")
        ));
    }
    let arguments = document
        .get("ProgramArguments")
        .and_then(Value::as_array)
        .and_then(|arguments| {
            arguments
                .iter()
                .map(|argument| argument.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        });
    match arguments {
        Some(arguments) => program_arguments_problem(&arguments, install_root),
        None => Some("ProgramArguments is not a list of strings.".to_string()),
    }
}

fn expected_program_arguments(install_root: &Path) -> [String; 4] {
    [
        install_root.join("bin/oore").display().to_string(),
        "update-supervisor".to_string(),
        "--request-file".to_string(),
        install_root
            .join(UPDATE_REQUEST_DIR)
            .join(UPDATE_REQUEST_FILE)
            .display()
            .to_string(),
    ]
}

fn program_arguments_problem(arguments: &[String], install_root: &Path) -> Option<String> {
    let expected = expected_program_arguments(install_root);
    (arguments != expected).then(|| {
        format!(
            "ProgramArguments is `{}`, expected `{}`.",
            arguments.join(" "),
            expected.join(" ")
        )
    })
}

fn loaded_update_supervisor_problem(
    output: &str,
    install_root: &Path,
    service_user: &str,
    service_home: &Path,
) -> Option<String> {
    let mut program = None;
    let mut username = None;
    let mut arguments = Vec::new();
    let mut environment = HashMap::new();
    let mut in_arguments = false;
    let mut in_environment = false;
    for line in output.lines().map(str::trim) {
        if !in_arguments && !in_environment {
            if let Some(value) = line.strip_prefix("program = ") {
                program = Some(value.trim_matches('"'));
            } else if let Some(value) = line.strip_prefix("username = ") {
                username = Some(value.trim_matches('"'));
            }
        }
        if line == "arguments = {" {
            in_arguments = true;
        } else if line == "environment = {" {
            in_environment = true;
        } else if in_arguments && line == "}" {
            in_arguments = false;
        } else if in_environment && line == "}" {
            in_environment = false;
        } else if in_arguments && !line.is_empty() {
            arguments.push(line.trim_matches('"').to_string());
        } else if in_environment && let Some((key, value)) = line.split_once(" => ") {
            environment.insert(key, value.trim_matches('"'));
        }
    }
    let expected_program = install_root.join("bin/oore");
    let expected_program = expected_program.to_string_lossy();
    let root = install_root.to_string_lossy();
    let home = service_home.to_string_lossy();
    let mismatch = |name: &str, actual: Option<&str>, expected: &str| {
        (actual != Some(expected)).then(|| {
            format!(
                "{name} is `{}`, expected `{expected}`.",
                actual.unwrap_or("unset")
            )
        })
    };
    mismatch("program", program, &expected_program)
        .or_else(|| mismatch("username", username, service_user))
        .or_else(|| mismatch("HOME", environment.get("HOME").copied(), &home))
        .or_else(|| mismatch("PATH", environment.get("PATH").copied(), SERVICE_PATH))
        .or_else(|| {
            mismatch(
                "OORE_INSTALL_ROOT",
                environment.get("OORE_INSTALL_ROOT").copied(),
                &root,
            )
        })
        .or_else(|| program_arguments_problem(&arguments, install_root))
}

fn backend_profile_from_manifest(path: &Path) -> Result<Option<String>, String> {
    let contents = match fs::read(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Could not read the install manifest {}: {error}",
                path.display()
            ));
        }
    };
    let manifest: Value = serde_json::from_slice(&contents).map_err(|error| {
        format!(
            "The install manifest {} is invalid: {error}",
            path.display()
        )
    })?;
    manifest
        .get("profile")
        .and_then(Value::as_str)
        .map(|profile| Some(profile.to_string()))
        .ok_or_else(|| format!("The install manifest {} has no profile.", path.display()))
}

fn runner_service_problem(profile: Option<&str>, runner_service: &Path) -> Result<String, String> {
    match profile {
        Some("control-plane") => Ok("Not needed for the control-plane profile.".to_string()),
        Some("complete") | None => runner_service_arguments(runner_service)
            .and_then(|arguments| runner_program_arguments_problem(&arguments).map_or(Ok(()), Err))
            .map(|()| runner_service.display().to_string()),
        Some(profile) => Err(format!(
            "The install manifest has an unknown profile `{profile}`."
        )),
    }
}

fn runner_service_arguments(path: &Path) -> Result<Vec<String>, String> {
    if !path.is_file() {
        return Err(format!(
            "{} is missing, so the runner is not a boot-time service.",
            path.display()
        ));
    }
    let output = Command::new("/usr/bin/plutil")
        .args(["-extract", "ProgramArguments", "json", "-o", "-"])
        .arg(path)
        .output()
        .map_err(|error| format!("Could not run plutil: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "plutil could not read ProgramArguments from {}: {}",
            path.display(),
            command_diagnostic(&output)
        ));
    }
    serde_json::from_slice::<Vec<String>>(&output.stdout).map_err(|_| {
        format!(
            "{} ProgramArguments is not a list of strings.",
            path.display()
        )
    })
}

fn runner_program_arguments_problem(arguments: &[String]) -> Option<String> {
    let expected = [
        (0, "/bin/launchctl"),
        (1, "asuser"),
        (3, "/usr/bin/sudo"),
        (4, "-E"),
        (5, "-H"),
        (6, "-u"),
        (9, "runner"),
        (10, "start"),
    ];
    expected
        .iter()
        .any(|(index, value)| arguments.get(*index).map(String::as_str) != Some(*value))
        .then(|| {
            format!(
                "The runner starts with `{}`, not the managed `launchctl asuser … sudo -E -H -u … runner start` command; this is an older runner service.",
                arguments.join(" ")
            )
        })
}

/// The supervisor reloads `oored` with `sudo -n /bin/launchctl` when it has to
/// roll back. `sudo -l` only lists what is allowed, so this runs nothing.
fn recovery_privileges_check(service_user: &str) -> RuntimeUpdateCheck {
    const ID: &str = "recovery_privileges";
    const LABEL: &str = "Rollback privileges";
    if service_user == "root" {
        return pass(ID, LABEL, "Running as root.".to_string());
    }
    let commands = [
        vec![
            "bootout".to_string(),
            format!("system/{DAEMON_SERVICE_LABEL}"),
        ],
        vec![
            "bootstrap".to_string(),
            "system".to_string(),
            DAEMON_SERVICE_PLIST.to_string(),
        ],
        vec![
            "kickstart".to_string(),
            "-k".to_string(),
            format!("system/{DAEMON_SERVICE_LABEL}"),
        ],
    ];
    let denied = commands
        .iter()
        .filter(|arguments| {
            !Command::new("/usr/bin/sudo")
                .args(["-n", "-l", "/bin/launchctl"])
                .args(arguments.iter())
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        })
        .map(|arguments| format!("`launchctl {}`", arguments.join(" ")))
        .collect::<Vec<_>>();
    if denied.is_empty() {
        return pass(
            ID,
            LABEL,
            format!("{service_user} may reload oored with `sudo -n /bin/launchctl`."),
        );
    }
    RuntimeUpdateCheck {
        id: ID.to_string(),
        label: LABEL.to_string(),
        status: RuntimeUpdateCheckStatus::Warn,
        detail: Some(format!(
            "{service_user} cannot run {} through `sudo -n`. Updates can still start, but if one fails while oored restarts, the supervisor cannot reload oored itself.",
            denied.join(", ")
        )),
        fix: Some(format!(
            "Run `oore setup` again from Terminal on the backend Mac to grant the rollback rule in /etc/sudoers.d/oore-update-recovery. Until then, if an update fails during restart, run `sudo launchctl kickstart -k system/{DAEMON_SERVICE_LABEL}`."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOADED: &str = r#"system/build.oore.oore-updater = {
    program = /Users/appbuilder/.oore/bin/oore
    arguments = {
        /Users/appbuilder/.oore/bin/oore
        update-supervisor
        --request-file
        /Users/appbuilder/.oore/run/runtime-update-queue/request.json
    }
    environment = {
        HOME => /Users/appbuilder
        PATH => /opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin
        OORE_INSTALL_ROOT => /Users/appbuilder/.oore
        XPC_SERVICE_NAME => build.oore.oore-updater
    }
    username = appbuilder
}"#;

    fn root() -> &'static Path {
        Path::new("/Users/appbuilder/.oore")
    }

    fn home() -> &'static Path {
        Path::new("/Users/appbuilder")
    }

    fn definition() -> Value {
        serde_json::json!({
            "Label": "build.oore.oore-updater",
            "UserName": "appbuilder",
            "ProgramArguments": [
                "/Users/appbuilder/.oore/bin/oore",
                "update-supervisor",
                "--request-file",
                "/Users/appbuilder/.oore/run/runtime-update-queue/request.json"
            ],
            "WorkingDirectory": "/Users/appbuilder/.oore",
            "EnvironmentVariables": {
                "HOME": "/Users/appbuilder",
                "PATH": SERVICE_PATH,
                "OORE_INSTALL_ROOT": "/Users/appbuilder/.oore"
            },
            "Umask": 63,
            "RunAtLoad": false,
            "KeepAlive": false,
            "StandardOutPath": "/Users/appbuilder/.oore/logs/update-supervisor.log",
            "StandardErrorPath": "/Users/appbuilder/.oore/logs/update-supervisor.log"
        })
    }

    #[test]
    fn loaded_supervisor_names_the_field_that_differs() {
        assert_eq!(
            loaded_update_supervisor_problem(LOADED, root(), "appbuilder", home()),
            None
        );
        let foreign = LOADED.replace(
            "program = /Users/appbuilder/.oore/bin/oore",
            "program = /tmp/foreign",
        );
        assert_eq!(
            loaded_update_supervisor_problem(&foreign, root(), "appbuilder", home()),
            Some(
                "program is `/tmp/foreign`, expected `/Users/appbuilder/.oore/bin/oore`."
                    .to_string()
            )
        );
        assert_eq!(
            loaded_update_supervisor_problem(LOADED, root(), "root", home()),
            Some("username is `appbuilder`, expected `root`.".to_string())
        );
    }

    #[test]
    fn supervisor_definition_names_foreign_keys_and_identity() {
        let mut document = definition();
        assert_eq!(
            update_supervisor_document_problem(&document, root(), "appbuilder", home()),
            None
        );

        document["Program"] = serde_json::json!("/tmp/foreign");
        assert_eq!(
            update_supervisor_document_problem(&document, root(), "appbuilder", home()),
            Some("Unexpected keys: Program.".to_string())
        );
        document
            .as_object_mut()
            .expect("document")
            .remove("Program");

        document["UserName"] = serde_json::json!("root");
        assert_eq!(
            update_supervisor_document_problem(&document, root(), "appbuilder", home()),
            Some("UserName is `root`, expected `appbuilder`.".to_string())
        );
        document["UserName"] = serde_json::json!("appbuilder");

        document["EnvironmentVariables"]["PATH"] = serde_json::json!("/usr/bin:/bin");
        assert_eq!(
            update_supervisor_document_problem(&document, root(), "appbuilder", home()),
            Some(format!(
                "EnvironmentVariables: PATH is `/usr/bin:/bin`, expected `{SERVICE_PATH}`."
            ))
        );
    }

    #[test]
    fn runner_service_explains_an_older_command() {
        let managed = [
            "/bin/launchctl",
            "asuser",
            "501",
            "/usr/bin/sudo",
            "-E",
            "-H",
            "-u",
            "appbuilder",
            "/Users/appbuilder/.oore/bin/oore",
            "runner",
            "start",
        ]
        .map(str::to_string);
        assert_eq!(runner_program_arguments_problem(&managed), None);
        let older = ["/Users/appbuilder/.oore/bin/oore", "runner", "start"].map(str::to_string);
        assert!(
            runner_program_arguments_problem(&older)
                .is_some_and(|problem| problem.contains("older runner service"))
        );
        assert_eq!(
            runner_service_problem(Some("control-plane"), Path::new("/missing")),
            Ok("Not needed for the control-plane profile.".to_string())
        );
        assert!(runner_service_problem(Some("mystery"), Path::new("/missing")).is_err());
    }

    #[test]
    fn readiness_ignores_warnings_but_not_failures() {
        let warn = RuntimeUpdateCheck {
            status: RuntimeUpdateCheckStatus::Warn,
            ..pass("recovery_privileges", "Rollback privileges", String::new())
        };
        assert!(preflight(vec![warn.clone()]).ready);
        let failed = preflight(vec![
            warn,
            fail(
                "updater_loaded",
                "Update supervisor loaded",
                String::new(),
                "fix",
            ),
        ]);
        assert!(!failed.ready);
        assert_eq!(
            first_failure(&failed).map(|check| check.id.as_str()),
            Some("updater_loaded")
        );
    }
}
