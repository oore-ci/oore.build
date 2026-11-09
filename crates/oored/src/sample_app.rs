//! A sample Flutter app to try Oore with before touching your own code.
//!
//! On a Local Only instance the backend runs `flutter create` into its data
//! folder, commits the result to a new Git repository, and adds it as an app
//! like any local repository. The first build then shows a green build and
//! an install page in minutes.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use oore_contract::{ApiError, CreateProjectRequest, CreateProjectResponse, RuntimeMode};
use tracing::{error, warn};

use crate::AppState;
use crate::extractors::AuthUser;
use crate::util::{api_err, resolve_oored_data_dir};

/// `flutter create` without `pub get` takes seconds; a first run of a new
/// Flutter install can take longer while it sets itself up.
const CREATE_TIMEOUT: Duration = Duration::from_secs(180);

const SAMPLE_PROJECT_NAME: &str = "oore_sample";

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

async fn run(program: &str, args: &[&str], dir: &Path) -> Result<(), String> {
    let mut command = tokio::process::Command::new(program);
    command
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(CREATE_TIMEOUT, command.output())
        .await
        .map_err(|_| {
            format!(
                "{program} did not finish in {} seconds",
                CREATE_TIMEOUT.as_secs()
            )
        })?
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => format!("{program} is not on the backend's PATH"),
            _ => format!("could not run {program}: {error}"),
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let last = stderr.lines().rev().find(|line| !line.trim().is_empty());
    Err(format!(
        "{program} {} failed{}",
        args.first().copied().unwrap_or_default(),
        last.map(|line| format!(": {}", line.trim()))
            .unwrap_or_default()
    ))
}

/// Writes the sample app into `dir` and commits it on `main`.
async fn write_sample(dir: &Path) -> Result<(), String> {
    let parent = dir.parent().ok_or("the sample folder has no parent")?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let target = dir.to_string_lossy().into_owned();
    run(
        "flutter",
        &[
            "create",
            "--no-pub",
            "--project-name",
            SAMPLE_PROJECT_NAME,
            "--org",
            "build.oore.sample",
            "--platforms",
            "android,ios",
            "--description",
            "A Flutter app Oore created to show a first build.",
            &target,
        ],
        parent,
    )
    .await?;
    run("git", &["init", "--quiet", "--initial-branch", "main"], dir).await?;
    run("git", &["add", "--all"], dir).await?;
    run(
        "git",
        &[
            "-c",
            "user.name=Oore",
            "-c",
            "user.email=sample@oore.build",
            "commit",
            "--quiet",
            "--message",
            "Flutter sample app",
        ],
        dir,
    )
    .await
}

fn sample_dir() -> Result<PathBuf, String> {
    let root = resolve_oored_data_dir().map_err(|error| error.to_string())?;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    Ok(root
        .join("samples")
        .join(format!("{SAMPLE_PROJECT_NAME}_{}", &suffix[..8])))
}

/// `POST /v1/projects/sample` — create a sample Flutter app and add it.
pub async fn create_sample_project(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<(StatusCode, Json<CreateProjectResponse>)> {
    auth.require_admin_or_above()?;
    let mode = crate::instance_settings::load_runtime_mode(&state.db)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to load runtime mode");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to determine runtime mode",
            )
        })?;
    if mode != RuntimeMode::Local {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "mode_restricted",
            "The sample app is a local repository, so it is only available in Local Only mode",
        ));
    }

    let dir = sample_dir()
        .map_err(|message| api_err(StatusCode::INTERNAL_SERVER_ERROR, "sample_failed", message))?;
    if let Err(message) = write_sample(&dir).await {
        warn!(dir = %dir.display(), %message, "could not create the sample app");
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(api_err(
            StatusCode::CONFLICT,
            "sample_failed",
            format!("Oore could not create the sample app: {message}. Add your own app instead."),
        ));
    }

    let request = CreateProjectRequest {
        name: "Sample app".to_string(),
        description: Some("A new Flutter app Oore created to show a first build.".to_string()),
        repository_id: None,
        local_repository_path: Some(dir.to_string_lossy().into_owned()),
        default_branch: Some("main".to_string()),
        auto_add_pipelines: None,
    };
    let created = crate::projects::create_project(State(state), auth, Json(request)).await;
    if created.is_err() {
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
    created
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_missing_tool_names_itself() {
        let error = run("oore-no-such-tool", &["create"], Path::new("."))
            .await
            .unwrap_err();
        assert_eq!(error, "oore-no-such-tool is not on the backend's PATH");
    }

    #[tokio::test]
    async fn a_failing_tool_reports_its_last_line() {
        let error = run("git", &["no-such-command"], Path::new("."))
            .await
            .unwrap_err();
        assert!(error.starts_with("git no-such-command failed: "), "{error}");
    }
}
