//! Config files Oore found in a project's repository, cached per branch head.
//!
//! Pipeline health and the "Found in repository" list read this cache, so
//! listing pipelines never calls the source provider. Pushes that touch a
//! config path refresh it off the webhook's hot path, and on the default
//! branch a new valid file no pipeline uses can become a pipeline.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, Pipeline, PipelineHealth, PipelineHealthState, RepositoryConfigFile,
    RepositoryConfigFilePipeline, RepositoryConfigFilesResponse, RepositoryWorkflowPreview,
};
use sqlx::Row;
use tracing::{error, info, warn};

use crate::AppState;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::repository_workflows::{
    config_path_candidates, discover_at, load_project_source, resolve_source_branch,
};
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

/// Refreshes are rare and short; one at a time keeps two of them from both
/// adding a pipeline for the same new file.
static REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A config file as the cache holds it.
#[derive(Debug, Clone)]
pub(crate) struct CachedFile {
    pub path: String,
    pub valid: bool,
    pub errors: Vec<String>,
    pub workflows: Vec<String>,
    pub seen_at: i64,
}

/// What Oore last read on one branch.
#[derive(Debug, Clone)]
pub(crate) struct BranchCache {
    pub branch: String,
    /// The commit the files were read at; `None` until a read succeeds.
    pub sha: Option<String>,
    pub scanned_at: i64,
    pub truncated: bool,
    pub error: Option<String>,
    pub files: Vec<CachedFile>,
}

impl BranchCache {
    /// The file a pipeline with `config_path` builds from, trying the legacy
    /// `.oore.yml` for `.oore.yaml` like the runner.
    pub(crate) fn resolve(&self, config_path: &str) -> Option<&CachedFile> {
        config_path_candidates(config_path)
            .into_iter()
            .find_map(|path| self.files.iter().find(|file| file.path == path))
    }

    /// A pipeline's health from this cache. `None` before the first read.
    pub(crate) fn health(
        &self,
        config_path: &str,
        workflow: Option<&str>,
    ) -> Option<PipelineHealth> {
        let sha = self.sha.clone()?;
        let branch = &self.branch;
        let (state, message, path) = match self.resolve(config_path) {
            None => (
                PipelineHealthState::FileMissing,
                Some(format!(
                    "{} is not on {branch}, so builds use the settings saved in Oore",
                    config_path.trim()
                )),
                None,
            ),
            Some(file) if !file.valid => (
                PipelineHealthState::FileInvalid,
                Some(match file.errors.first() {
                    Some(error) => format!("{} on {branch} is invalid: {error}", file.path),
                    None => format!("{} on {branch} is invalid", file.path),
                }),
                Some(file.path.clone()),
            ),
            Some(file) => match workflow {
                Some(workflow) if !file.workflows.iter().any(|name| name == workflow) => (
                    PipelineHealthState::WorkflowMissing,
                    Some(if file.workflows.is_empty() {
                        format!(
                            "{} on {branch} defines no workflows, but the pipeline builds workflow {workflow}",
                            file.path
                        )
                    } else {
                        format!(
                            "{} on {branch} no longer defines workflow {workflow}; it defines [{}]",
                            file.path,
                            file.workflows.join(", ")
                        )
                    }),
                    Some(file.path.clone()),
                ),
                _ => (PipelineHealthState::Ok, None, Some(file.path.clone())),
            },
        };
        Some(PipelineHealth {
            state,
            message,
            path,
            branch: branch.clone(),
            commit_sha: sha,
            checked_at: self.scanned_at,
        })
    }
}

/// The branch the cache tracks: the project's default branch, else the
/// repository's.
pub(crate) async fn default_branch(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    let branch: Option<Option<String>> = sqlx::query_scalar(
        "SELECT COALESCE(NULLIF(TRIM(p.default_branch), ''), NULLIF(TRIM(r.default_branch), '')) \
         FROM projects p LEFT JOIN integration_repositories r ON r.id = p.repository_id \
         WHERE p.id = ?1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(branch.flatten())
}

pub(crate) async fn load_cache(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    branch: &str,
) -> Result<Option<BranchCache>, sqlx::Error> {
    let Some(scan) = sqlx::query(
        "SELECT sha, truncated, error, scanned_at FROM repository_config_scans \
         WHERE project_id = ?1 AND branch = ?2",
    )
    .bind(project_id)
    .bind(branch)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let rows = sqlx::query(
        "SELECT path, valid, workflows, errors, seen_at FROM repository_config_files \
         WHERE project_id = ?1 AND branch = ?2",
    )
    .bind(project_id)
    .bind(branch)
    .fetch_all(pool)
    .await?;
    let mut files = rows
        .iter()
        .map(|row| CachedFile {
            path: row.get("path"),
            valid: row.get::<i64, _>("valid") != 0,
            errors: serde_json::from_str(&row.get::<String, _>("errors")).unwrap_or_default(),
            workflows: serde_json::from_str(&row.get::<String, _>("workflows")).unwrap_or_default(),
            seen_at: row.get("seen_at"),
        })
        .collect::<Vec<_>>();
    files.sort_by(|a, b| file_order(&a.path).cmp(&file_order(&b.path)));
    Ok(Some(BranchCache {
        branch: branch.to_string(),
        sha: scan.get("sha"),
        scanned_at: scan.get("scanned_at"),
        truncated: scan.get::<i64, _>("truncated") != 0,
        error: scan.get("error"),
        files,
    }))
}

/// `.oore.yaml` and `.oore.yml` first, then root files, then `.oore/`.
fn file_order(path: &str) -> (u8, &str) {
    let rank = match path {
        ".oore.yaml" => 0,
        ".oore.yml" => 1,
        _ if !path.contains('/') => 2,
        _ => 3,
    };
    (rank, path)
}

/// The default branch's cache for a project, or `None` when there is none.
async fn default_branch_cache(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> Result<Option<BranchCache>, sqlx::Error> {
    match default_branch(pool, project_id).await? {
        Some(branch) => load_cache(pool, project_id, &branch).await,
        None => Ok(None),
    }
}

/// Fill in `health` on pipelines of one project, from the cache only.
/// Best effort: a store error leaves health absent.
pub(crate) async fn attach_health(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    pipelines: &mut [Pipeline],
) {
    let cache = match default_branch_cache(pool, project_id).await {
        Ok(cache) => cache,
        Err(e) => {
            warn!(error = %e, project_id, "failed to load the repository config cache");
            return;
        }
    };
    let Some(cache) = cache else { return };
    for pipeline in pipelines {
        pipeline.health = cache.health(&pipeline.config_path, pipeline.workflow.as_deref());
    }
}

/// A pipeline name for a config file: `.oore.release.yaml` is `Release`,
/// `.oore/ios/beta.yaml` is `Ios beta`, and `.oore.yaml` is `Default`.
pub(crate) fn suggested_pipeline_name(path: &str) -> String {
    let stem = path
        .strip_suffix(".yaml")
        .or_else(|| path.strip_suffix(".yml"))
        .unwrap_or(path);
    let stem = stem
        .strip_prefix(".oore/")
        .or_else(|| stem.strip_prefix(".oore."))
        .unwrap_or_else(|| if stem == ".oore" { "" } else { stem });
    let words = stem
        .split(['/', '.', '-', '_', ' '])
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Default".to_string(),
    }
}

/// Whether a changed file is one discovery reads.
fn is_candidate_path(path: &str) -> bool {
    let yaml = path.ends_with(".yaml") || path.ends_with(".yml");
    yaml && ((path.starts_with(".oore.") && !path.contains('/'))
        || path
            .strip_prefix(".oore/")
            .is_some_and(|rest| rest.matches('/').count() <= 1))
}

/// Whether a push may have changed the config files: it touches a candidate
/// path, or the provider did not list its files.
pub(crate) fn push_touches_config_files(changed_files: Option<&[String]>) -> bool {
    changed_files.is_none_or(|files| files.iter().any(|path| is_candidate_path(path)))
}

/// What a refresh did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RefreshOutcome {
    /// The project has no default branch, or the push was to another branch.
    Skipped,
    /// The cache already held this commit.
    Unchanged,
    /// The cache was rewritten; pipelines auto-added from new files.
    Refreshed { added_pipelines: Vec<String> },
}

async fn record_scan_error(pool: &sqlx::SqlitePool, project_id: &str, branch: &str, error: &str) {
    let result = sqlx::query(
        "INSERT INTO repository_config_scans (project_id, branch, sha, truncated, error, scanned_at) \
         VALUES (?1, ?2, NULL, 0, ?3, ?4) \
         ON CONFLICT (project_id, branch) DO UPDATE SET error = excluded.error, scanned_at = excluded.scanned_at",
    )
    .bind(project_id)
    .bind(branch)
    .bind(error)
    .bind(now_unix())
    .execute(pool)
    .await;
    if let Err(e) = result {
        warn!(error = %e, project_id, "failed to record a repository config scan error");
    }
}

fn store_error(e: sqlx::Error) -> (StatusCode, Json<ApiError>) {
    error!(error = %e, "repository config cache store error");
    api_err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "store_error",
        "Failed to update the repository config cache",
    )
}

/// Read the project's default branch and rewrite its cache. `pushed` is a
/// `(branch, commit)` a webhook reported; a push to another branch is
/// skipped. Without `force`, a commit the cache already holds is not read
/// again.
pub(crate) async fn refresh(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    project_id: &str,
    pushed: Option<(&str, &str)>,
    force: bool,
) -> Result<RefreshOutcome, (StatusCode, Json<ApiError>)> {
    let _guard = REFRESH_LOCK.lock().await;
    let Some(branch) = default_branch(pool, project_id)
        .await
        .map_err(store_error)?
    else {
        return Ok(RefreshOutcome::Skipped);
    };
    if pushed.is_some_and(|(pushed_branch, _)| pushed_branch != branch) {
        return Ok(RefreshOutcome::Skipped);
    }
    let source = load_project_source(pool, project_id).await?;
    let sha = match pushed {
        Some((_, sha)) => sha.to_string(),
        None => match resolve_source_branch(pool, encryption_key, &source, &branch).await {
            Ok(sha) => sha,
            Err((status, body)) => {
                record_scan_error(pool, project_id, &branch, &body.0.error).await;
                return Err((status, body));
            }
        },
    };
    let previous = load_cache(pool, project_id, &branch)
        .await
        .map_err(store_error)?;
    if !force
        && previous.as_ref().is_some_and(|cache| {
            cache.sha.as_deref() == Some(sha.as_str()) && cache.error.is_none()
        })
    {
        return Ok(RefreshOutcome::Unchanged);
    }
    let (found, truncated) = match discover_at(pool, encryption_key, &source, &sha, None).await {
        Ok(found) => found,
        Err((status, body)) => {
            record_scan_error(pool, project_id, &branch, &body.0.error).await;
            return Err((status, body));
        }
    };
    let previous_seen: HashMap<String, i64> = previous
        .as_ref()
        .map(|cache| {
            cache
                .files
                .iter()
                .map(|file| (file.path.clone(), file.seen_at))
                .collect()
        })
        .unwrap_or_default();
    let now = now_unix();
    write_cache(
        pool,
        project_id,
        &branch,
        &sha,
        truncated,
        &found,
        &previous_seen,
        now,
    )
    .await
    .map_err(store_error)?;
    let cache = load_cache(pool, project_id, &branch)
        .await
        .map_err(store_error)?
        .expect("cache just written");
    resolve_legacy_config_paths(pool, project_id, &cache)
        .await
        .map_err(store_error)?;
    let added_pipelines = auto_add_pipelines(pool, project_id, &cache, &previous_seen)
        .await
        .map_err(store_error)?;
    info!(
        project_id,
        branch = %branch,
        commit = %sha,
        files = cache.files.len(),
        added = added_pipelines.len(),
        "repository config files refreshed"
    );
    Ok(RefreshOutcome::Refreshed { added_pipelines })
}

#[allow(clippy::too_many_arguments)]
async fn write_cache(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    branch: &str,
    sha: &str,
    truncated: bool,
    found: &[RepositoryWorkflowPreview],
    previous_seen: &HashMap<String, i64>,
    now: i64,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM repository_config_files WHERE project_id = ?1 AND branch = ?2")
        .bind(project_id)
        .bind(branch)
        .execute(&mut *tx)
        .await?;
    for file in found {
        sqlx::query(
            "INSERT INTO repository_config_files \
             (project_id, branch, path, sha, valid, workflows, errors, seen_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(project_id)
        .bind(branch)
        .bind(&file.path)
        .bind(sha)
        .bind(file.valid)
        .bind(serde_json::to_string(&file.workflows).unwrap_or_else(|_| "[]".into()))
        .bind(serde_json::to_string(&file.errors).unwrap_or_else(|_| "[]".into()))
        .bind(previous_seen.get(&file.path).copied().unwrap_or(now))
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO repository_config_scans (project_id, branch, sha, truncated, error, scanned_at) \
         VALUES (?1, ?2, ?3, ?4, NULL, ?5) \
         ON CONFLICT (project_id, branch) DO UPDATE SET sha = excluded.sha, \
             truncated = excluded.truncated, error = NULL, scanned_at = excluded.scanned_at",
    )
    .bind(project_id)
    .bind(branch)
    .bind(sha)
    .bind(truncated)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// Pipelines from before the config path was always explicit name
/// `.oore.yaml` and still read `.oore.yml`. Write the file the default
/// branch has, once.
async fn resolve_legacy_config_paths(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    cache: &BranchCache,
) -> Result<(), sqlx::Error> {
    let path = match cache.resolve(".oore.yaml") {
        Some(file) => file.path.as_str(),
        None => ".oore.yaml",
    };
    sqlx::query(
        "UPDATE pipelines SET config_path = ?1, config_path_explicit = 1 \
         WHERE project_id = ?2 AND config_path_explicit = 0",
    )
    .bind(path)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Add a pipeline for each valid file that is new since the last read and
/// that no pipeline uses, when the project asks for it.
async fn auto_add_pipelines(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    cache: &BranchCache,
    previous_seen: &HashMap<String, i64>,
) -> Result<Vec<String>, sqlx::Error> {
    let enabled: bool =
        sqlx::query_scalar("SELECT auto_add_pipelines != 0 FROM projects WHERE id = ?1")
            .bind(project_id)
            .fetch_optional(pool)
            .await?
            .unwrap_or(false);
    if !enabled {
        return Ok(Vec::new());
    }
    let used = used_paths(pool, project_id, cache).await?;
    let mut added = Vec::new();
    for file in &cache.files {
        if !file.valid || previous_seen.contains_key(&file.path) || used.contains(&file.path) {
            continue;
        }
        let name = suggested_pipeline_name(&file.path);
        let id =
            crate::pipelines::insert_pipeline_for_file(pool, project_id, &name, &file.path).await?;
        info!(project_id, pipeline_id = %id, path = %file.path, "pipeline added from a repository config file");
        added.push(id);
    }
    Ok(added)
}

/// Pipelines of a project with the cached file each builds from.
async fn pipelines_by_file(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    cache: &BranchCache,
) -> Result<HashMap<String, Vec<RepositoryConfigFilePipeline>>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, name, config_path FROM pipelines WHERE project_id = ?1 ORDER BY created_at, id",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    let mut by_file: HashMap<String, Vec<RepositoryConfigFilePipeline>> = HashMap::new();
    for row in &rows {
        if let Some(file) = cache.resolve(&row.get::<String, _>("config_path")) {
            by_file
                .entry(file.path.clone())
                .or_default()
                .push(RepositoryConfigFilePipeline {
                    id: row.get("id"),
                    name: row.get("name"),
                });
        }
    }
    Ok(by_file)
}

async fn used_paths(
    pool: &sqlx::SqlitePool,
    project_id: &str,
    cache: &BranchCache,
) -> Result<HashSet<String>, sqlx::Error> {
    Ok(pipelines_by_file(pool, project_id, cache)
        .await?
        .into_keys()
        .collect())
}

/// Refresh one project in the background, logging failures.
pub(crate) fn spawn_refresh(pool: sqlx::SqlitePool, encryption_key: Vec<u8>, project_id: String) {
    tokio::spawn(async move {
        if let Err((status, body)) = refresh(&pool, &encryption_key, &project_id, None, false).await
        {
            warn!(project_id = %project_id, status = %status, error = %body.0.error, "repository config refresh failed");
        }
    });
}

/// After a push, refresh each project linked to the repository whose
/// default branch it updated. Runs in the background and never fails the
/// delivery.
pub(crate) fn spawn_push_refresh(
    pool: sqlx::SqlitePool,
    encryption_key: Vec<u8>,
    integration_id: String,
    repo_full_name: String,
    branch: String,
    commit_sha: String,
) {
    tokio::spawn(async move {
        refresh_after_push(
            &pool,
            &encryption_key,
            &integration_id,
            &repo_full_name,
            &branch,
            &commit_sha,
        )
        .await;
    });
}

pub(crate) async fn refresh_after_push(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    integration_id: &str,
    repo_full_name: &str,
    branch: &str,
    commit_sha: &str,
) {
    let projects: Vec<String> = match sqlx::query_scalar(
        "SELECT p.id FROM projects p \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN integration_installations i ON i.id = r.installation_id \
         WHERE i.integration_id = ?1 AND r.full_name = ?2",
    )
    .bind(integration_id)
    .bind(repo_full_name)
    .fetch_all(pool)
    .await
    {
        Ok(projects) => projects,
        Err(e) => {
            warn!(error = %e, "failed to find projects for a repository config refresh");
            return;
        }
    };
    for project_id in projects {
        if let Err((status, body)) = refresh(
            pool,
            encryption_key,
            &project_id,
            Some((branch, commit_sha)),
            false,
        )
        .await
        {
            warn!(project_id = %project_id, status = %status, error = %body.0.error, "repository config refresh after a push failed");
        }
    }
}

async fn response(
    pool: &sqlx::SqlitePool,
    project_id: String,
) -> Result<(RepositoryConfigFilesResponse, bool), sqlx::Error> {
    let branch = default_branch(pool, &project_id).await?;
    let cache = match &branch {
        Some(branch) => load_cache(pool, &project_id, branch).await?,
        None => None,
    };
    let Some(cache) = cache else {
        return Ok((
            RepositoryConfigFilesResponse {
                project_id,
                branch,
                commit_sha: None,
                scanned_at: None,
                error: None,
                truncated: false,
                files: Vec::new(),
            },
            false,
        ));
    };
    let mut by_file = pipelines_by_file(pool, &project_id, &cache).await?;
    let files = cache
        .files
        .iter()
        .map(|file| RepositoryConfigFile {
            path: file.path.clone(),
            valid: file.valid,
            errors: file.errors.clone(),
            workflows: file.workflows.clone(),
            suggested_name: suggested_pipeline_name(&file.path),
            pipelines: by_file.remove(&file.path).unwrap_or_default(),
            seen_at: file.seen_at,
        })
        .collect();
    Ok((
        RepositoryConfigFilesResponse {
            project_id,
            branch,
            commit_sha: cache.sha,
            scanned_at: Some(cache.scanned_at),
            error: cache.error,
            truncated: cache.truncated,
            files,
        },
        true,
    ))
}

async fn authorize(
    state: &AppState,
    auth: &AuthUser,
    project_id: &str,
    permission: ProjectPermission,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let effective = resolve_effective_project_role(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, permission)?;
    let exists: bool = sqlx::query_scalar("SELECT COUNT(*) > 0 FROM projects WHERE id = ?1")
        .bind(project_id)
        .fetch_one(&state.db)
        .await
        .map_err(store_error)?;
    if !exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Project not found",
        ));
    }
    Ok(())
}

/// `GET /v1/projects/{project_id}/repository-config-files` — config files on
/// the default branch as Oore last read them. The first request for a
/// project that was never read starts a read in the background.
pub async fn list_repository_config_files(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<RepositoryConfigFilesResponse> {
    authorize(&state, &auth, &project_id, ProjectPermission::Read).await?;
    let (body, scanned) = response(&state.db, project_id.clone())
        .await
        .map_err(store_error)?;
    if !scanned && body.branch.is_some() {
        spawn_refresh(state.db.clone(), state.encryption_key.to_vec(), project_id);
    }
    Ok(Json(body))
}

/// `POST /v1/projects/{project_id}/repository-config-files/refresh` — read
/// the default branch now. A failed read is reported in `error` beside the
/// files from the read before it.
pub async fn refresh_repository_config_files(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> ApiResult<RepositoryConfigFilesResponse> {
    authorize(
        &state,
        &auth,
        &project_id,
        ProjectPermission::ManagePipelines,
    )
    .await?;
    match refresh(&state.db, &state.encryption_key, &project_id, None, true).await {
        Ok(_) => {}
        // Server-side failures are recorded on the scan and shown in `error`.
        Err((status, body)) if status.is_server_error() || status == StatusCode::BAD_REQUEST => {
            warn!(project_id = %project_id, status = %status, error = %body.0.error, "repository config refresh failed");
        }
        Err(error) => return Err(error),
    }
    let (body, _) = response(&state.db, project_id).await.map_err(store_error)?;
    Ok(Json(body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_names_come_from_the_path() {
        assert_eq!(suggested_pipeline_name(".oore.release.yaml"), "Release");
        assert_eq!(suggested_pipeline_name(".oore.yaml"), "Default");
        assert_eq!(suggested_pipeline_name(".oore.yml"), "Default");
        assert_eq!(suggested_pipeline_name(".oore/ios-beta.yml"), "Ios beta");
        assert_eq!(
            suggested_pipeline_name(".oore/release/nightly.yaml"),
            "Release nightly"
        );
    }

    #[test]
    fn only_pushes_touching_config_paths_refresh() {
        let files = |paths: &[&str]| paths.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        assert!(push_touches_config_files(None));
        assert!(!push_touches_config_files(Some(&files(&["lib/main.dart"]))));
        assert!(push_touches_config_files(Some(&files(&[
            "README.md",
            ".oore.release.yaml"
        ]))));
        assert!(push_touches_config_files(Some(&files(&[".oore/ci/a.yml"]))));
        assert!(!push_touches_config_files(Some(&files(&[
            ".oore/a/b/c.yml"
        ]))));
        assert!(!push_touches_config_files(Some(&files(&[
            "app/.oore.yaml"
        ]))));
    }

    fn commit_files(repo: &std::path::Path, files: &[(&str, Option<&str>)]) -> String {
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(["-c", "user.name=Oore", "-c", "user.email=oore@example.com"])
                .args(args)
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
            String::from_utf8(output.stdout).unwrap().trim().to_string()
        };
        if !repo.join(".git").exists() {
            git(&["init", "-q", "-b", "main"]);
        }
        for (path, contents) in files {
            let full = repo.join(path);
            match contents {
                Some(contents) => {
                    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
                    std::fs::write(full, contents).unwrap();
                }
                None => std::fs::remove_file(full).unwrap(),
            }
        }
        git(&["add", "-A"]);
        git(&["commit", "-q", "--allow-empty", "-m", "change"]);
        git(&["rev-parse", "HEAD"])
    }

    async fn local_pool(repo: &std::path::Path, auto_add: bool) -> sqlx::SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let repo = repo.to_str().unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'lead@example.com', 'sub', 'owner', 0, 0)",
            "INSERT INTO integrations (id, provider, host_url, auth_mode, created_by, created_at, updated_at) \
             VALUES ('i1', 'local_git', 'local://filesystem', 'local_path', 'u1', 0, 0)",
            "INSERT INTO integration_installations (id, integration_id, external_id, account_name, created_at, updated_at) \
             VALUES ('inst1', 'i1', ?1, 'local', 0, 0)",
            "INSERT INTO integration_repositories (id, installation_id, external_id, full_name, default_branch, html_url, created_at, updated_at) \
             VALUES ('r1', 'inst1', ?1, 'app', 'main', ?1, 0, 0)",
            "INSERT INTO projects (id, name, repository_id, created_by, created_at, updated_at) \
             VALUES ('p1', 'App', 'r1', 'u1', 0, 0)",
        ] {
            sqlx::query(statement)
                .bind(repo)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE projects SET auto_add_pipelines = ?1")
            .bind(auto_add)
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    async fn pipelines(pool: &sqlx::SqlitePool) -> Vec<(String, String)> {
        sqlx::query_as("SELECT name, config_path FROM pipelines ORDER BY config_path")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    const V2: &str = "version: 2\ntargets:\n  android:\n    build: flutter build apk\nworkflows:\n  adhoc: {}\n  release: {}\n";

    #[tokio::test]
    async fn new_files_on_the_default_branch_become_pipelines() {
        let repo = tempfile::tempdir().unwrap();
        commit_files(
            repo.path(),
            &[
                (".oore.yaml", Some("version: 1\nplatforms: [android]\n")),
                (".oore.release.yaml", Some(V2)),
                (".oore/qa/nightly.yml", Some(V2)),
                (".oore/broken.yaml", Some("version: 2\nnope: 1\n")),
            ],
        );
        let pool = local_pool(repo.path(), true).await;
        let key = [0u8; 32];

        let outcome = refresh(&pool, &key, "p1", None, false).await.unwrap();
        let RefreshOutcome::Refreshed { added_pipelines } = outcome else {
            panic!("{outcome:?}");
        };
        assert_eq!(added_pipelines.len(), 3);
        assert_eq!(
            pipelines(&pool).await,
            [
                ("Release".to_string(), ".oore.release.yaml".to_string()),
                ("Default".to_string(), ".oore.yaml".to_string()),
                ("Qa nightly".to_string(), ".oore/qa/nightly.yml".to_string()),
            ]
        );
        assert_eq!(
            refresh(&pool, &key, "p1", None, false).await.unwrap(),
            RefreshOutcome::Unchanged
        );

        // A pipeline someone deleted is not added back for a file Oore has
        // already seen; a new file is added.
        sqlx::query("DELETE FROM pipelines WHERE config_path = '.oore.release.yaml'")
            .execute(&pool)
            .await
            .unwrap();
        let head = commit_files(repo.path(), &[(".oore.adhoc.yml", Some(V2))]);
        // A push to another branch is not the default branch's cache.
        assert_eq!(
            refresh(&pool, &key, "p1", Some(("develop", &head)), false)
                .await
                .unwrap(),
            RefreshOutcome::Skipped
        );
        refresh(&pool, &key, "p1", Some(("main", &head)), false)
            .await
            .unwrap();
        let names = pipelines(&pool).await;
        assert!(names.contains(&("Adhoc".to_string(), ".oore.adhoc.yml".to_string())));
        assert!(!names.iter().any(|(_, path)| path == ".oore.release.yaml"));

        let (body, scanned) = response(&pool, "p1".into()).await.unwrap();
        assert!(scanned);
        assert_eq!(body.commit_sha.as_deref(), Some(head.as_str()));
        let release = body
            .files
            .iter()
            .find(|file| file.path == ".oore.release.yaml")
            .unwrap();
        assert!(release.pipelines.is_empty());
        assert_eq!(release.workflows, ["adhoc", "release"]);
        let broken = body
            .files
            .iter()
            .find(|file| file.path == ".oore/broken.yaml")
            .unwrap();
        assert!(!broken.valid && !broken.errors.is_empty());
    }

    #[tokio::test]
    async fn auto_add_is_off_for_existing_projects() {
        let repo = tempfile::tempdir().unwrap();
        commit_files(repo.path(), &[(".oore.release.yaml", Some(V2))]);
        let pool = local_pool(repo.path(), false).await;
        refresh(&pool, &[0u8; 32], "p1", None, false).await.unwrap();
        assert!(pipelines(&pool).await.is_empty());
    }

    async fn health_of(
        pool: &sqlx::SqlitePool,
        config_path: &str,
        workflow: Option<&str>,
    ) -> Option<PipelineHealth> {
        default_branch_cache(pool, "p1")
            .await
            .unwrap()
            .and_then(|cache| cache.health(config_path, workflow))
    }

    #[tokio::test]
    async fn health_comes_from_the_cache() {
        let repo = tempfile::tempdir().unwrap();
        commit_files(
            repo.path(),
            &[
                (".oore.yml", Some(V2)),
                (".oore/broken.yaml", Some("version: 2\nnope: 1\n")),
            ],
        );
        let pool = local_pool(repo.path(), false).await;
        assert!(health_of(&pool, ".oore.yaml", None).await.is_none());
        // A pipeline from before config paths were explicit.
        sqlx::query(
            "INSERT INTO pipelines (id, project_id, name, config_path, config_path_explicit, created_at, updated_at) \
             VALUES ('legacy', 'p1', 'Legacy', '.oore.yaml', 0, 0, 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        refresh(&pool, &[0u8; 32], "p1", None, false).await.unwrap();

        let state = |health: Option<PipelineHealth>| health.unwrap().state;
        let ok = health_of(&pool, ".oore.yaml", Some("release"))
            .await
            .unwrap();
        assert_eq!(ok.state, PipelineHealthState::Ok);
        assert_eq!(ok.path.as_deref(), Some(".oore.yml"));
        assert_eq!(
            state(health_of(&pool, ".oore.yaml", Some("nightly")).await),
            PipelineHealthState::WorkflowMissing
        );
        assert_eq!(
            state(health_of(&pool, ".oore/broken.yaml", None).await),
            PipelineHealthState::FileInvalid
        );
        assert_eq!(
            state(health_of(&pool, ".oore/gone.yaml", None).await),
            PipelineHealthState::FileMissing
        );
        // The legacy pipeline now names the file the branch has.
        let (path, explicit): (String, i64) = sqlx::query_as(
            "SELECT config_path, config_path_explicit FROM pipelines WHERE id = 'legacy'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!((path.as_str(), explicit), (".oore.yml", 1));
    }
}
