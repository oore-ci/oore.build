use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

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

pub type RuntimeUpdateState = Arc<RwLock<RuntimeUpdateStatus>>;

pub fn new_state() -> RuntimeUpdateState {
    Arc::new(RwLock::new(RuntimeUpdateStatus {
        phase: RuntimeUpdatePhase::Idle,
        error: None,
        managed_service: PathBuf::from(SYSTEM_SERVICE_PLIST).is_file(),
    }))
}

pub async fn get_status(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<Json<RuntimeUpdateStatus>, (StatusCode, Json<ApiError>)> {
    auth.require_owner()?;
    Ok(Json(state.runtime_update.read().await.clone()))
}

pub async fn start_update(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<(StatusCode, Json<RuntimeUpdateStatus>), (StatusCode, Json<ApiError>)> {
    auth.require_owner()?;

    let oore = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("oore")))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            api_err(
                StatusCode::CONFLICT,
                "runtime_update_unavailable",
                "The installed oore updater was not found beside oored",
            )
        })?;

    {
        let mut status = state.runtime_update.write().await;
        if !status.managed_service {
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
        status.phase = RuntimeUpdatePhase::Updating;
        status.error = None;
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

    let update_state = state.runtime_update.clone();
    tokio::spawn(async move {
        let result = tokio::task::spawn_blocking(move || {
            Command::new(oore)
                .arg("update")
                .env("OORE_UPDATE_DEFER_DAEMON_RESTART", "1")
                .output()
        })
        .await;

        let output = match result {
            Ok(Ok(output)) if output.status.success() => {
                let mut status = update_state.write().await;
                status.phase = RuntimeUpdatePhase::Restarting;
                status.error = None;
                drop(status);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                std::process::exit(75);
            }
            Ok(Ok(output)) => String::from_utf8_lossy(&output.stderr).trim().to_string(),
            Ok(Err(error)) => error.to_string(),
            Err(error) => error.to_string(),
        };

        let mut status = update_state.write().await;
        status.phase = RuntimeUpdatePhase::Failed;
        status.error = Some(if output.is_empty() {
            "Backend update failed without diagnostic output".to_string()
        } else {
            output.chars().take(2_000).collect()
        });
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(state.runtime_update.read().await.clone()),
    ))
}
