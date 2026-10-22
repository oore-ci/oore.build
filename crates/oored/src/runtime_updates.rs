use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use oore_contract::runtime_update_preflight as preflight;
use oore_contract::{
    ApiError, DeferredRuntimeUpdateRequest, RuntimeUpdateCheck, RuntimeUpdateCheckStatus,
    RuntimeUpdatePhase, RuntimeUpdatePreflight, RuntimeUpdateStatus,
};
use tokio::sync::{Mutex, RwLock};

use crate::AppState;
use crate::extractors::AuthUser;
use crate::store::write_audit_log;
use crate::util::api_err;

const UPDATE_SERVICE: &str = "system/build.oore.oore-updater";
const UPDATE_STATUS_FILE: &str = ".runtime-update-status.json";
const UPDATE_REQUEST_DIR: &str = preflight::UPDATE_REQUEST_DIR;
const UPDATE_REQUEST_FILE: &str = preflight::UPDATE_REQUEST_FILE;
const PREFLIGHT_TTL: Duration = Duration::from_secs(30);
/// How long an update may go without progress before a stopped supervisor counts as gone.
const ABANDONED_UPDATE_GRACE: Duration = Duration::from_secs(120);
const ABANDONED_UPDATE_ERROR: &str = "The update supervisor stopped without reporting a result. Check logs/update-supervisor.log on the backend host, then try again.";
/// How long an update may go without progress when launchd cannot say whether
/// the supervisor runs. It outlasts the 30 minute runner drain plus the backup.
const UNCONFIRMED_UPDATE_TIMEOUT: Duration = Duration::from_secs(45 * 60);
const UNCONFIRMED_UPDATE_ERROR: &str = "The update made no progress for 45 minutes, and Oore could not confirm that the update supervisor is still running. Check logs/update-supervisor.log on the backend host, then try again.";

pub type RuntimeUpdateState = Arc<RwLock<RuntimeUpdateStatus>>;

static PREFLIGHT_CACHE: Mutex<Option<(Instant, RuntimeUpdatePreflight)>> = Mutex::const_new(None);

fn run_preflight_blocking() -> RuntimeUpdatePreflight {
    match install_root_from_current_exe() {
        Ok(install_root) => preflight::run(&install_root),
        Err(error) => RuntimeUpdatePreflight {
            ready: false,
            checked_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            checks: vec![RuntimeUpdateCheck {
                id: "installed_updater".to_string(),
                label: "Installed updater".to_string(),
                status: RuntimeUpdateCheckStatus::Fail,
                detail: Some(format!("{error:#}")),
                fix: Some(
                    "Reinstall Oore with the installer from Terminal on the backend Mac."
                        .to_string(),
                ),
            }],
        },
    }
}

/// Runs the update preflight off the async runtime, reusing a result younger
/// than [`PREFLIGHT_TTL`] unless `fresh` is set. The checks shell out to
/// `launchctl`, `plutil`, `id` and `sudo`, so status polls must not run them
/// every time.
async fn update_preflight(fresh: bool) -> RuntimeUpdatePreflight {
    let mut cache = PREFLIGHT_CACHE.lock().await;
    if !fresh
        && let Some((checked, preflight)) = cache.as_ref()
        && checked.elapsed() < PREFLIGHT_TTL
    {
        return preflight.clone();
    }
    let preflight = tokio::task::spawn_blocking(run_preflight_blocking)
        .await
        .unwrap_or_else(|error| RuntimeUpdatePreflight {
            ready: false,
            checked_at: 0,
            checks: vec![RuntimeUpdateCheck {
                id: "preflight".to_string(),
                label: "Update preflight".to_string(),
                status: RuntimeUpdateCheckStatus::Fail,
                detail: Some(format!("The preflight checks did not finish: {error}")),
                fix: Some("Run `oore doctor --update` on the backend Mac.".to_string()),
            }],
        });
    *cache = Some((Instant::now(), preflight.clone()));
    preflight
}

fn unavailable_message(preflight: &RuntimeUpdatePreflight) -> String {
    match preflight::first_failure(preflight) {
        Some(check) => {
            let mut message = format!("One-click backend update is unavailable: {}", check.label);
            if let Some(detail) = &check.detail {
                message.push_str(&format!(". {detail}"));
            }
            if let Some(fix) = &check.fix {
                message.push_str(&format!(" {fix}"));
            }
            message
        }
        None => "One-click backend update is unavailable".to_string(),
    }
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
    let status: RuntimeUpdateStatus = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid runtime update status {}", path.display()))?;
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

/// `managed_service` starts false; `get_status` fills it from the preflight.
fn initial_status() -> RuntimeUpdateStatus {
    let managed_service = false;
    match update_status_path().and_then(|path| read_persisted_status(&path)) {
        Ok(Some(status)) => RuntimeUpdateStatus {
            managed_service,
            ..status
        },
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

fn update_supervisor_running() -> Option<bool> {
    let output = Command::new("/bin/launchctl")
        .args(["print", UPDATE_SERVICE])
        .output()
        .ok()?;
    if !output.status.success() {
        return Some(false);
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.trim().starts_with("pid = ")),
    )
}

fn time_since_modified(path: &Path) -> Option<Duration> {
    let modified = fs::symlink_metadata(path).ok()?.modified().ok()?;
    Some(
        SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default(),
    )
}

/// The failure to report when an update looks abandoned, or `None` while it may
/// still be running.
fn abandoned_update_error(
    quiet_for: Option<Duration>,
    supervisor_running: Option<bool>,
) -> Option<&'static str> {
    let quiet_for = quiet_for?;
    match supervisor_running {
        Some(false) if quiet_for >= ABANDONED_UPDATE_GRACE => Some(ABANDONED_UPDATE_ERROR),
        None if quiet_for >= UNCONFIRMED_UPDATE_TIMEOUT => Some(UNCONFIRMED_UPDATE_ERROR),
        _ => None,
    }
}

/// Fail an update whose supervisor exited without reporting, and drop its queued request,
/// so the UI stops polling and the next click starts a fresh attempt.
fn reconcile_abandoned_update(
    install_root: &Path,
    supervisor_running: impl FnOnce() -> Option<bool>,
) -> anyhow::Result<()> {
    let status_path = install_root.join(UPDATE_STATUS_FILE);
    let request_path = install_root
        .join(UPDATE_REQUEST_DIR)
        .join(UPDATE_REQUEST_FILE);
    let in_flight = read_persisted_status(&status_path)?.filter(|status| {
        matches!(
            status.phase,
            RuntimeUpdatePhase::Updating | RuntimeUpdatePhase::Restarting
        )
    });
    let request_queued = fs::symlink_metadata(&request_path).is_ok();
    if in_flight.is_none() && !request_queued {
        return Ok(());
    }
    let quiet_for = [&status_path, &request_path]
        .into_iter()
        .filter_map(|path| time_since_modified(path))
        .min();
    let Some(error) = abandoned_update_error(quiet_for, supervisor_running()) else {
        return Ok(());
    };
    match fs::remove_file(&request_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to clear stale update request {}",
                    request_path.display()
                )
            });
        }
    }
    if let Some(mut status) = in_flight {
        status.phase = RuntimeUpdatePhase::Failed;
        status.error = Some(error.to_string());
        write_persisted_status(&status_path, &status)?;
    }
    Ok(())
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
    Ok(format!(
        "http://{}",
        crate::loopback_maintenance_addr(listen)
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
    // Reconciling may run `launchctl print`, so keep it off the async runtime.
    if let Ok(install_root) = install_root_from_current_exe()
        && let Err(error) = tokio::task::spawn_blocking(move || {
            reconcile_abandoned_update(&install_root, update_supervisor_running)
        })
        .await
        .map_err(anyhow::Error::from)
        .and_then(|result| result)
    {
        tracing::warn!(
            error = format!("{error:#}"),
            "failed to reconcile an abandoned backend update"
        );
    }
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
                    managed_service: false,
                };
            }
        }
    }
    let ready = update_preflight(false).await.ready;
    let mut status = state.runtime_update.write().await;
    status.managed_service = ready;
    Ok(Json(status.clone()))
}

pub async fn get_preflight(
    auth: AuthUser,
) -> Result<Json<RuntimeUpdatePreflight>, (StatusCode, Json<ApiError>)> {
    auth.require_owner()?;
    Ok(Json(update_preflight(false).await))
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
    reconcile_abandoned_update(&install_root, update_supervisor_running).map_err(|error| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "runtime_update_status_failed",
            format!("{error:#}"),
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
    let preflight = update_preflight(true).await;

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
        if !preflight.ready {
            status.managed_service = false;
            return Err(api_err(
                StatusCode::CONFLICT,
                "runtime_update_unmanaged",
                unavailable_message(&preflight),
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

    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "runtime_update_started",
        "system",
        Some("backend"),
        None,
    )
    .await;

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

    fn write_update_fixture(
        install_root: &Path,
        phase: RuntimeUpdatePhase,
        request_queued: bool,
        age: Duration,
    ) -> (PathBuf, PathBuf) {
        let status_path = install_root.join(UPDATE_STATUS_FILE);
        write_persisted_status(
            &status_path,
            &RuntimeUpdateStatus {
                phase,
                error: None,
                managed_service: true,
            },
        )
        .unwrap();
        let request_path = install_root
            .join(UPDATE_REQUEST_DIR)
            .join(UPDATE_REQUEST_FILE);
        fs::create_dir_all(request_path.parent().unwrap()).unwrap();
        if request_queued {
            fs::write(&request_path, b"{}").unwrap();
        }
        let modified = SystemTime::now() - age;
        for path in [&status_path, &request_path] {
            if let Ok(file) = fs::File::options().write(true).open(path) {
                file.set_modified(modified).unwrap();
            }
        }
        (status_path, request_path)
    }

    fn persisted_phase(path: &Path) -> (RuntimeUpdatePhase, Option<String>) {
        let status = read_persisted_status(path).unwrap().unwrap();
        (status.phase, status.error)
    }

    #[test]
    fn abandoned_update_fails_and_clears_the_queued_request() {
        let root = tempfile::tempdir().unwrap();
        let (status_path, request_path) = write_update_fixture(
            root.path(),
            RuntimeUpdatePhase::Updating,
            true,
            Duration::from_secs(600),
        );

        reconcile_abandoned_update(root.path(), || Some(false)).unwrap();

        assert_eq!(
            persisted_phase(&status_path),
            (
                RuntimeUpdatePhase::Failed,
                Some(ABANDONED_UPDATE_ERROR.to_string())
            )
        );
        assert!(!request_path.exists());
    }

    #[test]
    fn stale_request_is_cleared_after_a_reported_failure() {
        let root = tempfile::tempdir().unwrap();
        let (status_path, request_path) = write_update_fixture(
            root.path(),
            RuntimeUpdatePhase::Failed,
            true,
            Duration::from_secs(600),
        );

        reconcile_abandoned_update(root.path(), || Some(false)).unwrap();

        assert_eq!(persisted_phase(&status_path).0, RuntimeUpdatePhase::Failed);
        assert!(!request_path.exists());
    }

    #[test]
    fn an_hour_old_update_fails_when_the_supervisor_state_is_unknown() {
        let root = tempfile::tempdir().unwrap();
        let (status_path, request_path) = write_update_fixture(
            root.path(),
            RuntimeUpdatePhase::Updating,
            true,
            Duration::from_secs(60 * 60),
        );

        reconcile_abandoned_update(root.path(), || None).unwrap();

        assert_eq!(
            persisted_phase(&status_path),
            (
                RuntimeUpdatePhase::Failed,
                Some(UNCONFIRMED_UPDATE_ERROR.to_string())
            )
        );
        assert!(!request_path.exists());
    }

    #[test]
    fn running_recent_or_unknown_updates_are_left_alone() {
        for (age, supervisor_running) in [
            (Duration::from_secs(60 * 60), Some(true)),
            (Duration::from_secs(600), None),
            (Duration::from_secs(40 * 60), None),
            (Duration::from_secs(5), Some(false)),
        ] {
            let root = tempfile::tempdir().unwrap();
            let (status_path, request_path) =
                write_update_fixture(root.path(), RuntimeUpdatePhase::Restarting, true, age);

            reconcile_abandoned_update(root.path(), || supervisor_running).unwrap();

            assert_eq!(
                persisted_phase(&status_path),
                (RuntimeUpdatePhase::Restarting, None)
            );
            assert!(request_path.exists());
        }
    }
}
