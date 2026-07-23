use std::fs;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use oore_contract::{
    ApiError, DeferredRuntimeUpdateRequest, RuntimeUpdatePhase, RuntimeUpdateStatus,
};
use tokio::sync::RwLock;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::store::write_audit_log;
use crate::util::api_err;

const SYSTEM_SERVICE_PLIST: &str = "/Library/LaunchDaemons/build.oore.oored.plist";
const UPDATE_SERVICE_PLIST: &str = "/Library/LaunchDaemons/build.oore.oore-updater.plist";
const RUNNER_SERVICE_PLIST: &str = "/Library/LaunchDaemons/build.oore.oore-runner.plist";
const UPDATE_SERVICE: &str = "system/build.oore.oore-updater";
const UPDATE_STATUS_FILE: &str = ".runtime-update-status.json";
const UPDATE_REQUEST_DIR: &str = "run/runtime-update-queue";
const UPDATE_REQUEST_FILE: &str = "request.json";

pub type RuntimeUpdateState = Arc<RwLock<RuntimeUpdateStatus>>;

fn managed_service_installed() -> bool {
    cfg!(target_os = "macos")
        && Path::new(SYSTEM_SERVICE_PLIST).is_file()
        && Path::new(UPDATE_SERVICE_PLIST).is_file()
        && runner_service_is_update_ready(Path::new(RUNNER_SERVICE_PLIST))
}

fn runner_program_arguments_are_update_ready(arguments: &[String]) -> bool {
    let uses_alpha_wrapper = arguments.first().map(String::as_str) == Some("/bin/launchctl")
        && arguments.get(1).map(String::as_str) == Some("asuser")
        && arguments.get(3).map(String::as_str) == Some("/usr/bin/sudo")
        && arguments.get(4).map(String::as_str) == Some("-E")
        && arguments.get(5).map(String::as_str) == Some("-H")
        && arguments.get(6).map(String::as_str) == Some("-u");
    !uses_alpha_wrapper
        && arguments.get(1).map(String::as_str) == Some("runner")
        && arguments.get(2).map(String::as_str) == Some("start")
}

fn runner_service_is_update_ready(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let Ok(output) = Command::new("/usr/bin/plutil")
        .args(["-extract", "ProgramArguments", "json", "-o", "-"])
        .arg(path)
        .output()
    else {
        return false;
    };
    output.status.success()
        && serde_json::from_slice::<Vec<String>>(&output.stdout)
            .is_ok_and(|arguments| runner_program_arguments_are_update_ready(&arguments))
}

fn install_root_from_current_exe() -> anyhow::Result<PathBuf> {
    let executable = std::env::current_exe().context("failed to locate oored")?;
    executable
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("installed oored path has no install root")
}

fn update_status_path() -> anyhow::Result<PathBuf> {
    Ok(install_root_from_current_exe()?.join(UPDATE_STATUS_FILE))
}

fn read_persisted_status(path: &Path) -> anyhow::Result<Option<RuntimeUpdateStatus>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!("failed to read runtime update status {}", path.display())
            });
        }
    };
    let mut status: RuntimeUpdateStatus = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid runtime update status {}", path.display()))?;
    status.managed_service = managed_service_installed();
    Ok(Some(status))
}

fn write_persisted_status(path: &Path, status: &RuntimeUpdateStatus) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path
        .parent()
        .context("runtime update status has no parent")?;
    fs::create_dir_all(parent)?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = parent.join(format!(
        ".{UPDATE_STATUS_FILE}.{}-{nonce}.tmp",
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    if let Err(error) = (|| -> anyhow::Result<()> {
        serde_json::to_writer_pretty(&mut file, status)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })() {
        let _ = fs::remove_file(&temporary);
        return Err(error).with_context(|| {
            format!("failed to publish runtime update status {}", path.display())
        });
    }
    Ok(())
}

fn initial_status() -> RuntimeUpdateStatus {
    let managed_service = managed_service_installed();
    match update_status_path().and_then(|path| read_persisted_status(&path)) {
        Ok(Some(mut status)) => {
            status.managed_service = managed_service;
            status
        }
        Ok(None) => RuntimeUpdateStatus {
            phase: RuntimeUpdatePhase::Idle,
            error: None,
            managed_service,
        },
        Err(error) => RuntimeUpdateStatus {
            phase: RuntimeUpdatePhase::Failed,
            error: Some(format!(
                "Could not read the last backend update status: {error:#}"
            )),
            managed_service,
        },
    }
}

pub fn new_state() -> RuntimeUpdateState {
    Arc::new(RwLock::new(initial_status()))
}

fn refresh_managed_service_state(status: &mut RuntimeUpdateStatus, managed_service: bool) {
    status.managed_service = managed_service;
}

fn process_listen_address() -> anyhow::Result<SocketAddr> {
    let arguments = std::env::args().collect::<Vec<_>>();
    let from_arguments = arguments.iter().enumerate().find_map(|(index, argument)| {
        if argument == "--listen" {
            return arguments.get(index + 1).cloned();
        }
        argument.strip_prefix("--listen=").map(str::to_string)
    });
    from_arguments
        .or_else(|| std::env::var("OORED_LISTEN_ADDR").ok())
        .unwrap_or_else(|| "127.0.0.1:8787".to_string())
        .parse()
        .context("failed to determine the daemon loopback address for update verification")
}

fn loopback_daemon_url() -> anyhow::Result<String> {
    let listen = process_listen_address()?;
    let loopback = match listen.ip() {
        IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
    };
    Ok(format!(
        "http://{}",
        SocketAddr::new(loopback, listen.port())
    ))
}

struct DeferredUpdateInvocation {
    parent_pid: u32,
    database: PathBuf,
    key: PathBuf,
    daemon_url: String,
    status: PathBuf,
}

impl From<DeferredUpdateInvocation> for DeferredRuntimeUpdateRequest {
    fn from(invocation: DeferredUpdateInvocation) -> Self {
        Self {
            parent_pid: invocation.parent_pid,
            database: invocation.database,
            key: invocation.key,
            daemon_url: invocation.daemon_url,
            status: invocation.status,
        }
    }
}

fn write_update_request(path: &Path, request: &DeferredRuntimeUpdateRequest) -> anyhow::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;

    let parent = path
        .parent()
        .context("runtime update request has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".{UPDATE_REQUEST_FILE}.{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    if let Err(error) = (|| -> anyhow::Result<()> {
        serde_json::to_writer(&mut file, request)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })() {
        let _ = fs::remove_file(&temporary);
        return Err(error).context("failed to publish the runtime update request");
    }
    Ok(())
}

fn start_update_supervisor(
    path: &Path,
    request: &DeferredRuntimeUpdateRequest,
) -> anyhow::Result<()> {
    write_update_request(path, request)?;
    let output = Command::new("/bin/launchctl")
        .args(["kickstart", UPDATE_SERVICE])
        .output()
        .context("failed to start the managed update supervisor")?;
    if !output.status.success() {
        let _ = fs::remove_file(path);
        let diagnostic = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let suffix = if diagnostic.is_empty() {
            String::new()
        } else {
            format!(": {diagnostic}")
        };
        anyhow::bail!("managed update supervisor did not start{suffix}");
    }
    Ok(())
}

pub async fn get_status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<RuntimeUpdateStatus>, (StatusCode, Json<ApiError>)> {
    auth.require_owner()?;
    if let Ok(path) = update_status_path() {
        match read_persisted_status(&path) {
            Ok(Some(status)) => *state.runtime_update.write().await = status,
            Ok(None) => {}
            Err(error) => {
                *state.runtime_update.write().await = RuntimeUpdateStatus {
                    phase: RuntimeUpdatePhase::Failed,
                    error: Some(format!(
                        "Could not read the last backend update status: {error:#}"
                    )),
                    managed_service: managed_service_installed(),
                };
            }
        }
    }
    let mut status = state.runtime_update.write().await;
    refresh_managed_service_state(&mut status, managed_service_installed());
    Ok(Json(status.clone()))
}

pub async fn start_update(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<RuntimeUpdateStatus>), (StatusCode, Json<ApiError>)> {
    auth.require_owner()?;

    let install_root = install_root_from_current_exe().map_err(|error| {
        api_err(
            StatusCode::CONFLICT,
            "runtime_update_unavailable",
            error.to_string(),
        )
    })?;
    let oore = install_root.join("bin/oore");
    if !oore.is_file() {
        return Err(api_err(
            StatusCode::CONFLICT,
            "runtime_update_unavailable",
            "The installed oore updater was not found beside oored",
        ));
    }
    let status_path = install_root.join(UPDATE_STATUS_FILE);
    let database = {
        let store = state.store.lock().await;
        store.path().to_path_buf()
    };
    let key = crate::crypto::resolve_key_path().map_err(|error| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "runtime_update_unavailable",
            error.to_string(),
        )
    })?;
    let daemon_url = loopback_daemon_url().map_err(|error| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "runtime_update_unavailable",
            error.to_string(),
        )
    })?;
    let request_path = install_root
        .join(UPDATE_REQUEST_DIR)
        .join(UPDATE_REQUEST_FILE);
    if request_path.exists() {
        return Err(api_err(
            StatusCode::CONFLICT,
            "runtime_update_in_progress",
            "A backend update request is already queued",
        ));
    }
    fs::create_dir_all(request_path.parent().expect("request path has a parent")).map_err(
        |error| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "runtime_update_unavailable",
                format!("Failed to prepare the update queue: {error}"),
            )
        },
    )?;
    let invocation = DeferredUpdateInvocation {
        parent_pid: std::process::id(),
        database,
        key,
        daemon_url,
        status: status_path.clone(),
    };

    {
        let mut status = state.runtime_update.write().await;
        if let Some(persisted) = read_persisted_status(&status_path).map_err(|error| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "runtime_update_status_failed",
                error.to_string(),
            )
        })? {
            *status = persisted;
        }
        if !managed_service_installed() {
            status.managed_service = false;
            return Err(api_err(
                StatusCode::CONFLICT,
                "runtime_update_unmanaged",
                "Backend updates from the web require the current managed macOS services; run the installer once from Terminal to finish or repair service setup",
            ));
        }
        if matches!(
            status.phase,
            RuntimeUpdatePhase::Updating | RuntimeUpdatePhase::Restarting
        ) {
            return Err(api_err(
                StatusCode::CONFLICT,
                "runtime_update_in_progress",
                "A backend update is already in progress",
            ));
        }
        let next = RuntimeUpdateStatus {
            phase: RuntimeUpdatePhase::Updating,
            error: None,
            managed_service: true,
        };
        write_persisted_status(&status_path, &next).map_err(|error| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "runtime_update_status_failed",
                error.to_string(),
            )
        })?;
        *status = next;
    }

    {
        let store = state.store.lock().await;
        let _ = write_audit_log(
            store.pool(),
            Some(&auth.0.user_id),
            "runtime_update_started",
            "system",
            Some("backend"),
            None,
        )
        .await;
    }

    let result = tokio::task::spawn_blocking(move || {
        start_update_supervisor(&request_path, &invocation.into())
    })
    .await;
    let failure = match result {
        Ok(Ok(())) => None,
        Ok(Err(error)) => Some(error.to_string()),
        Err(error) => Some(error.to_string()),
    };
    if let Some(failure) = failure {
        let diagnostic = if failure.is_empty() {
            "Could not queue the backend update".to_string()
        } else {
            failure.chars().take(2_000).collect()
        };
        let mut status = state.runtime_update.write().await;
        status.phase = RuntimeUpdatePhase::Failed;
        status.error = Some(diagnostic.clone());
        let _ = write_persisted_status(&status_path, &status);
        return Err(api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "runtime_update_start_failed",
            diagnostic,
        ));
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(state.runtime_update.read().await.clone()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_runner_service_is_ready_for_web_updates() {
        let arguments = [
            "/Users/appbuilder/.oore/bin/oore",
            "runner",
            "start",
            "--config",
            "/Users/appbuilder/.oore/managed-runner.json",
        ]
        .map(str::to_string);

        assert!(runner_program_arguments_are_update_ready(&arguments));
    }

    #[test]
    fn wrapped_alpha_runner_service_requires_installer_repair() {
        let arguments = [
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

        assert!(!runner_program_arguments_are_update_ready(&arguments));
    }

    #[test]
    fn status_refresh_discovers_services_installed_after_daemon_start() {
        let mut status = RuntimeUpdateStatus {
            phase: RuntimeUpdatePhase::Idle,
            error: None,
            managed_service: false,
        };

        refresh_managed_service_state(&mut status, true);

        assert!(status.managed_service);
    }

    #[test]
    fn queued_request_carries_the_complete_deferred_update_contract() {
        let invocation = DeferredUpdateInvocation {
            parent_pid: 42,
            database: "/data/oore.db".into(),
            key: "/data/encryption.key".into(),
            daemon_url: "http://127.0.0.1:8787".into(),
            status: "/opt/oore/.runtime-update-status.json".into(),
        };
        let request = DeferredRuntimeUpdateRequest::from(invocation);
        assert_eq!(request.parent_pid, 42);
        assert_eq!(request.database, Path::new("/data/oore.db"));
        assert_eq!(request.key, Path::new("/data/encryption.key"));
        assert_eq!(request.daemon_url, "http://127.0.0.1:8787");
        assert_eq!(
            request.status,
            Path::new("/opt/oore/.runtime-update-status.json")
        );
    }

    #[test]
    fn queued_request_is_private_and_complete_before_publish() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(UPDATE_REQUEST_FILE);
        let request = DeferredRuntimeUpdateRequest {
            parent_pid: 42,
            database: "/data/oore.db".into(),
            key: "/data/encryption.key".into(),
            daemon_url: "http://127.0.0.1:8787".into(),
            status: "/opt/oore/.runtime-update-status.json".into(),
        };

        write_update_request(&path, &request).unwrap();

        assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
        let persisted: DeferredRuntimeUpdateRequest =
            serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(persisted.parent_pid, 42);
        assert_eq!(persisted.status, request.status);
    }
}
