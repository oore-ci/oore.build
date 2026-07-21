use std::ffi::OsString;
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
use oore_contract::{ApiError, RuntimeUpdatePhase, RuntimeUpdateStatus};
use tokio::sync::RwLock;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::store::write_audit_log;
use crate::util::api_err;

const SYSTEM_SERVICE_PLIST: &str = "/Library/LaunchDaemons/build.oore.oored.plist";
const UPDATE_STATUS_FILE: &str = ".runtime-update-status.json";
const UPDATE_LOG_FILE: &str = "runtime-update.log";

pub type RuntimeUpdateState = Arc<RwLock<RuntimeUpdateStatus>>;

fn managed_service_installed() -> bool {
    cfg!(target_os = "macos") && Path::new(SYSTEM_SERVICE_PLIST).is_file()
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
    oore: PathBuf,
    parent_pid: u32,
    database: PathBuf,
    key: PathBuf,
    daemon_url: String,
    status: PathBuf,
    log: PathBuf,
    label: String,
}

impl DeferredUpdateInvocation {
    fn launchctl_args(&self) -> Vec<OsString> {
        vec![
            "submit".into(),
            "-l".into(),
            self.label.clone().into(),
            "-o".into(),
            self.log.as_os_str().to_owned(),
            "-e".into(),
            self.log.as_os_str().to_owned(),
            "--".into(),
            "/usr/bin/env".into(),
            "OORE_UPDATE_DEFER_DAEMON_RESTART=1".into(),
            self.oore.as_os_str().to_owned(),
            "update".into(),
            "--deferred-parent-pid".into(),
            self.parent_pid.to_string().into(),
            "--deferred-state-file".into(),
            self.database.as_os_str().to_owned(),
            "--deferred-key-file".into(),
            self.key.as_os_str().to_owned(),
            "--deferred-daemon-url".into(),
            self.daemon_url.clone().into(),
            "--deferred-status-file".into(),
            self.status.as_os_str().to_owned(),
        ]
    }
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
    Ok(Json(state.runtime_update.read().await.clone()))
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
    let logs = install_root.join("logs");
    fs::create_dir_all(&logs).map_err(|error| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "runtime_update_unavailable",
            format!("Failed to prepare the update log: {error}"),
        )
    })?;
    let invocation = DeferredUpdateInvocation {
        oore,
        parent_pid: std::process::id(),
        database,
        key,
        daemon_url,
        status: status_path.clone(),
        log: logs.join(UPDATE_LOG_FILE),
        label: format!(
            "build.oore.runtime-update.{}.{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ),
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
                "Backend updates from the web require the managed macOS launchd service",
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
        Command::new("/bin/launchctl")
            .args(invocation.launchctl_args())
            .output()
    })
    .await;
    let failure = match result {
        Ok(Ok(output)) if output.status.success() => None,
        Ok(Ok(output)) => Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        Ok(Err(error)) => Some(error.to_string()),
        Err(error) => Some(error.to_string()),
    };
    if let Some(failure) = failure {
        let diagnostic = if failure.is_empty() {
            "launchd rejected the backend update job".to_string()
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
    fn launchd_job_carries_the_complete_deferred_update_contract() {
        let invocation = DeferredUpdateInvocation {
            oore: "/opt/oore/bin/oore".into(),
            parent_pid: 42,
            database: "/data/oore.db".into(),
            key: "/data/encryption.key".into(),
            daemon_url: "http://127.0.0.1:8787".into(),
            status: "/opt/oore/.runtime-update-status.json".into(),
            log: "/opt/oore/logs/runtime-update.log".into(),
            label: "build.oore.runtime-update.test".into(),
        };
        let arguments = invocation
            .launchctl_args()
            .into_iter()
            .map(|value| value.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        for required in [
            "OORE_UPDATE_DEFER_DAEMON_RESTART=1",
            "--deferred-parent-pid",
            "--deferred-state-file",
            "--deferred-key-file",
            "--deferred-daemon-url",
            "--deferred-status-file",
        ] {
            assert!(arguments.iter().any(|argument| argument == required));
        }
    }
}
