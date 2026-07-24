use std::collections::HashSet;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, DiscoverRepositoryWorkflowsResponse, RepositoryWorkflowExecutionPreview,
    RepositoryWorkflowPreview, ScmProvider, parse_repository_pipeline_yaml,
    validate_repository_config_path,
};
use serde::Deserialize;
use sqlx::Row;
use tracing::error;

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::util::api_err;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const MAX_WORKFLOW_FILES: usize = 16;
const MAX_WORKFLOW_BYTES: usize = 128 * 1024;
const MAX_DIRECTORY_RESPONSE_BYTES: usize = 512 * 1024;

#[derive(Debug, Deserialize)]
pub struct DiscoverRepositoryWorkflowsQuery {
    #[serde(rename = "ref")]
    reference: Option<String>,
    path: Option<String>,
}

struct ProjectSource {
    provider: ScmProvider,
    integration_id: String,
    installation_external_id: String,
    repository_external_id: String,
    full_name: String,
    host_url: String,
    auth_mode: String,
    local_path: Option<String>,
    default_reference: String,
}

fn validate_reference(reference: &str) -> Result<String, (StatusCode, Json<ApiError>)> {
    let reference = reference.trim();
    if reference.is_empty()
        || reference.len() > 255
        || reference.starts_with('-')
        || reference.chars().any(char::is_control)
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_ref",
            "ref must be a non-empty branch, tag, or commit name up to 255 characters",
        ));
    }
    Ok(reference.to_string())
}

fn workflow_candidates(
    explicit_path: Option<&str>,
    directory_paths: impl IntoIterator<Item = String>,
) -> Result<(Vec<String>, bool), (StatusCode, Json<ApiError>)> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for path in explicit_path
        .into_iter()
        .map(str::to_string)
        .chain([".oore.yaml".to_string(), ".oore.yml".to_string()])
        .chain(directory_paths)
    {
        if validate_repository_config_path(&path).is_err() || !seen.insert(path.clone()) {
            continue;
        }
        if paths.len() == MAX_WORKFLOW_FILES {
            return Ok((paths, true));
        }
        paths.push(path);
    }
    Ok((paths, false))
}

fn preview(path: String, raw: &str) -> RepositoryWorkflowPreview {
    match parse_repository_pipeline_yaml(raw) {
        Ok(config) => RepositoryWorkflowPreview {
            path,
            valid: true,
            errors: Vec::new(),
            execution: Some(RepositoryWorkflowExecutionPreview {
                platforms: config.platforms,
                flutter_version: config.flutter_version,
                commands: config.commands,
                platform_build_args: config.platform_build_args,
                platform_commands: config.platform_commands,
                env_keys: config.env.into_iter().map(|entry| entry.key).collect(),
                artifact_patterns: config.artifact_patterns,
            }),
        },
        Err(error) => RepositoryWorkflowPreview {
            path,
            valid: false,
            errors: if error.starts_with("YAML parse error:") {
                let location = error
                    .rfind(" at line ")
                    .map(|index| &error[index..])
                    .unwrap_or_default();
                vec![format!(
                    "YAML is invalid{location}; check its syntax and value types"
                )]
            } else {
                error.lines().map(str::to_string).collect()
            },
            execution: None,
        },
    }
}

async fn response_bytes(
    mut response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, (StatusCode, Json<ApiError>)> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(api_err(
            StatusCode::BAD_GATEWAY,
            "source_response_too_large",
            "The source provider returned more data than workflow discovery allows",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| {
        error!(error = %e, "failed to read source provider response");
        api_err(
            StatusCode::BAD_GATEWAY,
            "source_api_error",
            "Failed to read the source provider response",
        )
    })? {
        if bytes.len() + chunk.len() > max_bytes {
            return Err(api_err(
                StatusCode::BAD_GATEWAY,
                "source_response_too_large",
                "The source provider returned more data than workflow discovery allows",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn provider_error(provider: &str, status: reqwest::StatusCode) -> (StatusCode, Json<ApiError>) {
    api_err(
        StatusCode::BAD_GATEWAY,
        "source_api_error",
        format!("{provider} returned {status} while discovering repository workflows"),
    )
}

fn gitlab_api_url(
    host_url: &str,
    suffix: &[&str],
) -> Result<url::Url, (StatusCode, Json<ApiError>)> {
    let mut url = url::Url::parse(host_url).map_err(|_| {
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_source_host",
            "The linked GitLab host is invalid",
        )
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "invalid_source_host",
            "The linked GitLab host is invalid",
        ));
    }
    url.set_query(None);
    url.set_fragment(None);
    url.path_segments_mut()
        .map_err(|_| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "invalid_source_host",
                "The linked GitLab host is invalid",
            )
        })?
        .clear()
        .extend(["api", "v4"])
        .extend(suffix.iter().copied());
    Ok(url)
}

fn source_client() -> Result<&'static reqwest::Client, (StatusCode, Json<ApiError>)> {
    crate::integrations::scm_http_client().map_err(|e| {
        error!(error = %e, "failed to build workflow discovery client");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "http_client_error",
            "Failed to create the source provider client",
        )
    })
}

async fn gitlab_request(
    client: &reqwest::Client,
    source: &ProjectSource,
    token: &str,
    url: url::Url,
) -> Result<reqwest::Response, (StatusCode, Json<ApiError>)> {
    let request = client.get(url).header("User-Agent", "oore-ci");
    let request = if source.auth_mode == "oauth_app" {
        request.header("Authorization", format!("Bearer {token}"))
    } else {
        request.header("PRIVATE-TOKEN", token)
    };
    request.send().await.map_err(|e| {
        error!(error = %e, "GitLab workflow discovery request failed");
        api_err(
            StatusCode::BAD_GATEWAY,
            "source_api_error",
            "Failed to communicate with GitLab",
        )
    })
}

async fn discover_gitlab(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    source: &ProjectSource,
    reference: &str,
    explicit_path: Option<&str>,
) -> Result<(Vec<RepositoryWorkflowPreview>, bool), (StatusCode, Json<ApiError>)> {
    let encrypted_token: Option<String> = sqlx::query_scalar(
        "SELECT encrypted_value FROM integration_credentials \
         WHERE integration_id = ?1 AND credential_type = 'access_token'",
    )
    .bind(&source.integration_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, "failed to load GitLab workflow discovery credentials");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load source credentials",
        )
    })?;
    let token = crypto::decrypt(
        encrypted_token.as_deref().ok_or_else(|| {
            api_err(
                StatusCode::CONFLICT,
                "missing_credentials",
                "GitLab credentials are missing; reconnect the source",
            )
        })?,
        encryption_key,
    )
    .map_err(|e| {
        error!(error = %e, "failed to decrypt GitLab workflow discovery credentials");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "encryption_error",
            "Failed to decrypt source credentials",
        )
    })?;

    let client = source_client()?;
    let mut tree_url = gitlab_api_url(
        &source.host_url,
        &[
            "projects",
            &source.repository_external_id,
            "repository",
            "tree",
        ],
    )?;
    tree_url
        .query_pairs_mut()
        .append_pair("path", ".oore")
        .append_pair("ref", reference)
        .append_pair("per_page", "100");
    let tree_response = gitlab_request(client, source, &token, tree_url).await?;
    let tree_truncated = tree_response
        .headers()
        .get("x-next-page")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| !value.is_empty());
    let directory_paths = if tree_response.status() == StatusCode::NOT_FOUND {
        Vec::new()
    } else if tree_response.status().is_success() {
        #[derive(Deserialize)]
        struct TreeEntry {
            name: String,
            #[serde(rename = "type")]
            kind: String,
        }
        let body = response_bytes(tree_response, MAX_DIRECTORY_RESPONSE_BYTES).await?;
        serde_json::from_slice::<Vec<TreeEntry>>(&body)
            .map_err(|e| {
                error!(error = %e, "failed to parse GitLab repository tree");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "source_parse_error",
                    "Failed to parse the GitLab repository tree",
                )
            })?
            .into_iter()
            .filter(|entry| {
                entry.kind == "blob"
                    && (entry.name.ends_with(".yaml") || entry.name.ends_with(".yml"))
            })
            .map(|entry| format!(".oore/{}", entry.name))
            .collect()
    } else {
        return Err(provider_error("GitLab", tree_response.status()));
    };

    let (paths, truncated) = workflow_candidates(explicit_path, directory_paths)?;
    let mut workflows = Vec::new();
    for path in paths {
        let mut file_url = gitlab_api_url(
            &source.host_url,
            &[
                "projects",
                &source.repository_external_id,
                "repository",
                "files",
                &path,
                "raw",
            ],
        )?;
        file_url.query_pairs_mut().append_pair("ref", reference);
        let response = gitlab_request(client, source, &token, file_url).await?;
        if response.status() == StatusCode::NOT_FOUND {
            continue;
        }
        if !response.status().is_success() {
            return Err(provider_error("GitLab", response.status()));
        }
        let body = response_bytes(response, MAX_WORKFLOW_BYTES).await?;
        let raw = std::str::from_utf8(&body).map_err(|_| {
            api_err(
                StatusCode::BAD_GATEWAY,
                "invalid_workflow_encoding",
                format!("{path} must be UTF-8 text"),
            )
        })?;
        workflows.push(preview(path, raw));
    }
    Ok((workflows, truncated || tree_truncated))
}

async fn github_get(
    client: &reqwest::Client,
    source: &ProjectSource,
    token: &str,
    path: &str,
    reference: &str,
    raw: bool,
) -> Result<reqwest::Response, (StatusCode, Json<ApiError>)> {
    let url = github_contents_url(&source.full_name, path, reference);
    client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .header(
            "Accept",
            if raw {
                "application/vnd.github.raw+json"
            } else {
                "application/vnd.github+json"
            },
        )
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "oore-ci")
        .send()
        .await
        .map_err(|e| {
            error!(error = %e, "GitHub workflow discovery request failed");
            api_err(
                StatusCode::BAD_GATEWAY,
                "source_api_error",
                "Failed to communicate with GitHub",
            )
        })
}

fn github_contents_url(full_name: &str, path: &str, reference: &str) -> url::Url {
    let mut url = url::Url::parse("https://api.github.com").expect("static GitHub API URL");
    url.path_segments_mut()
        .expect("GitHub API URL supports path segments")
        .extend(["repos"])
        .extend(full_name.split('/'))
        .extend(["contents"])
        .extend(path.split('/'));
    url.query_pairs_mut().append_pair("ref", reference);
    url
}

async fn discover_github(
    pool: &sqlx::SqlitePool,
    encryption_key: &[u8],
    source: &ProjectSource,
    reference: &str,
    explicit_path: Option<&str>,
) -> Result<(Vec<RepositoryWorkflowPreview>, bool), (StatusCode, Json<ApiError>)> {
    let client = source_client()?;
    let token = crate::integrations::github::load_installation_access_token(
        client,
        pool,
        encryption_key,
        &source.integration_id,
        &source.installation_external_id,
    )
    .await?;
    let tree_response = github_get(client, source, &token, ".oore", reference, false).await?;
    let directory_paths = if tree_response.status() == StatusCode::NOT_FOUND {
        Vec::new()
    } else if tree_response.status().is_success() {
        #[derive(Deserialize)]
        struct ContentEntry {
            name: String,
            #[serde(rename = "type")]
            kind: String,
        }
        let body = response_bytes(tree_response, MAX_DIRECTORY_RESPONSE_BYTES).await?;
        serde_json::from_slice::<Vec<ContentEntry>>(&body)
            .map_err(|e| {
                error!(error = %e, "failed to parse GitHub repository contents");
                api_err(
                    StatusCode::BAD_GATEWAY,
                    "source_parse_error",
                    "Failed to parse the GitHub repository contents",
                )
            })?
            .into_iter()
            .filter(|entry| {
                entry.kind == "file"
                    && (entry.name.ends_with(".yaml") || entry.name.ends_with(".yml"))
            })
            .map(|entry| format!(".oore/{}", entry.name))
            .collect()
    } else {
        return Err(provider_error("GitHub", tree_response.status()));
    };

    let (paths, truncated) = workflow_candidates(explicit_path, directory_paths)?;
    let mut workflows = Vec::new();
    for path in paths {
        let response = github_get(client, source, &token, &path, reference, true).await?;
        if response.status() == StatusCode::NOT_FOUND {
            continue;
        }
        if !response.status().is_success() {
            return Err(provider_error("GitHub", response.status()));
        }
        let body = response_bytes(response, MAX_WORKFLOW_BYTES).await?;
        let raw = std::str::from_utf8(&body).map_err(|_| {
            api_err(
                StatusCode::BAD_GATEWAY,
                "invalid_workflow_encoding",
                format!("{path} must be UTF-8 text"),
            )
        })?;
        workflows.push(preview(path, raw));
    }
    Ok((workflows, truncated))
}

fn read_limited_stdout(
    mut command: Command,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, std::io::Error> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut output = Vec::new();
    child
        .stdout
        .take()
        .expect("piped stdout")
        .take((max_bytes + 1) as u64)
        .read_to_end(&mut output)?;
    let status = child.wait()?;
    if !status.success() {
        return Ok(None);
    }
    if output.len() > max_bytes {
        return Err(std::io::Error::other("git output exceeded discovery limit"));
    }
    Ok(Some(output))
}

async fn discover_local(
    repo_path: &str,
    reference: &str,
    explicit_path: Option<&str>,
) -> Result<(Vec<RepositoryWorkflowPreview>, bool), (StatusCode, Json<ApiError>)> {
    let repo_path = repo_path.to_string();
    let reference = reference.to_string();
    let explicit_path = explicit_path.map(str::to_string);
    tokio::task::spawn_blocking(move || {
        let commit_output = Command::new("git")
            .args([
                "-C",
                repo_path.as_str(),
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{reference}^{{commit}}"),
            ])
            .output()
            .map_err(|e| {
                error!(error = %e, "failed to resolve local workflow discovery ref");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "git_error",
                    "Failed to inspect the local repository",
                )
            })?;
        if !commit_output.status.success() {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_ref",
                format!("ref '{reference}' was not found in the linked repository"),
            ));
        }
        let commit = String::from_utf8(commit_output.stdout)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "git_error",
                    "Git returned an invalid commit identifier",
                )
            })?;

        let mut tree = Command::new("git");
        tree.args([
            "-C",
            repo_path.as_str(),
            "ls-tree",
            "--name-only",
            &format!("{commit}:.oore"),
        ]);
        let directory_paths = match read_limited_stdout(tree, MAX_DIRECTORY_RESPONSE_BYTES) {
            Ok(Some(bytes)) => String::from_utf8_lossy(&bytes)
                .lines()
                .filter(|name| name.ends_with(".yaml") || name.ends_with(".yml"))
                .map(|name| format!(".oore/{name}"))
                .collect(),
            Ok(None) => Vec::new(),
            Err(_) => {
                return Err(api_err(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "repository_tree_too_large",
                    "The repository contains too many workflow files to discover safely",
                ));
            }
        };
        let (paths, truncated) = workflow_candidates(explicit_path.as_deref(), directory_paths)?;
        let mut workflows = Vec::new();
        for path in paths {
            let object = format!("{commit}:{path}");
            let size = Command::new("git")
                .args(["-C", repo_path.as_str(), "cat-file", "-s", &object])
                .output()
                .map_err(|e| {
                    error!(error = %e, "failed to inspect local workflow file");
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "git_error",
                        "Failed to inspect the local repository",
                    )
                })?;
            if !size.status.success() {
                continue;
            }
            let size = String::from_utf8_lossy(&size.stdout)
                .trim()
                .parse::<usize>()
                .unwrap_or(usize::MAX);
            if size > MAX_WORKFLOW_BYTES {
                return Err(api_err(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "workflow_too_large",
                    format!("{path} exceeds the {MAX_WORKFLOW_BYTES} byte discovery limit"),
                ));
            }
            let mut cat = Command::new("git");
            cat.args(["-C", repo_path.as_str(), "cat-file", "blob", &object]);
            let body = read_limited_stdout(cat, MAX_WORKFLOW_BYTES)
                .map_err(|_| {
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "git_error",
                        "Failed to read the local repository workflow",
                    )
                })?
                .ok_or_else(|| {
                    api_err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "git_error",
                        "Failed to read the local repository workflow",
                    )
                })?;
            let raw = std::str::from_utf8(&body).map_err(|_| {
                api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_workflow_encoding",
                    format!("{path} must be UTF-8 text"),
                )
            })?;
            workflows.push(preview(path, raw));
        }
        Ok((workflows, truncated))
    })
    .await
    .map_err(|e| {
        error!(error = %e, "local workflow discovery task failed");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_error",
            "Failed to inspect the local repository",
        )
    })?
}

async fn load_project_source(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> Result<ProjectSource, (StatusCode, Json<ApiError>)> {
    let row = sqlx::query(
        "SELECT i.id AS integration_id, i.provider, i.host_url, i.auth_mode, \
                inst.external_id AS installation_external_id, r.external_id AS repository_external_id, \
                r.full_name, r.default_branch AS repository_default_branch, \
                p.default_branch AS project_default_branch \
         FROM projects p \
         JOIN integration_repositories r ON r.id = p.repository_id \
         JOIN integration_installations inst ON inst.id = r.installation_id \
         JOIN integrations i ON i.id = inst.integration_id \
         WHERE p.id = ?1 AND i.status = 'active'",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!(error = %e, project_id, "failed to load project source");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to load the project source",
        )
    })?
    .ok_or_else(|| {
        api_err(
            StatusCode::NOT_FOUND,
            "source_not_found",
            "The project does not have an active linked repository",
        )
    })?;
    let provider_raw: String = row.get("provider");
    let provider = provider_raw.parse::<ScmProvider>().map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "unsupported_provider",
            "The linked repository provider does not support workflow discovery",
        )
    })?;
    let repository_external_id: String = row.get("repository_external_id");
    let project_default: Option<String> = row.get("project_default_branch");
    let repository_default: Option<String> = row.get("repository_default_branch");
    let default_reference = project_default
        .or(repository_default)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            api_err(
                StatusCode::CONFLICT,
                "missing_default_branch",
                "Set a project default branch before discovering repository workflows",
            )
        })?;
    Ok(ProjectSource {
        provider,
        integration_id: row.get("integration_id"),
        installation_external_id: row.get("installation_external_id"),
        repository_external_id: repository_external_id.clone(),
        full_name: row.get("full_name"),
        host_url: row.get("host_url"),
        auth_mode: row.get("auth_mode"),
        local_path: (provider == ScmProvider::LocalGit).then_some(repository_external_id),
        default_reference,
    })
}

/// `GET /v1/projects/{project_id}/repository-workflows` — discover repository-owned configs.
pub async fn discover_repository_workflows(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
    Query(query): Query<DiscoverRepositoryWorkflowsQuery>,
) -> ApiResult<DiscoverRepositoryWorkflowsResponse> {
    let pool = state.db.clone();
    let effective = resolve_effective_project_role(
        &pool,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::ManagePipelines)?;

    let source = load_project_source(&pool, &project_id).await?;
    let reference = validate_reference(
        query
            .reference
            .as_deref()
            .unwrap_or(&source.default_reference),
    )?;
    let explicit_path = query.path.as_deref().map(str::trim);
    if let Some(path) = explicit_path
        && let Err(error) = validate_repository_config_path(path)
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_config_path",
            format!("path {error}"),
        ));
    }

    let (workflows, truncated) = match source.provider {
        ScmProvider::Github => {
            discover_github(
                &pool,
                &state.encryption_key,
                &source,
                &reference,
                explicit_path,
            )
            .await?
        }
        ScmProvider::Gitlab => {
            discover_gitlab(
                &pool,
                &state.encryption_key,
                &source,
                &reference,
                explicit_path,
            )
            .await?
        }
        ScmProvider::LocalGit => {
            discover_local(
                source.local_path.as_deref().expect("local path loaded"),
                &reference,
                explicit_path,
            )
            .await?
        }
    };

    // A missing ref and a valid repository with no workflow files both produce
    // provider 404s. Resolve only the empty result so the latter stays a normal
    // empty discovery response while invalid refs get an actionable error.
    if workflows.is_empty() {
        match source.provider {
            ScmProvider::Github => {
                crate::integrations::github::resolve_branch_commit(
                    &pool,
                    &state.encryption_key,
                    &source.integration_id,
                    &source.installation_external_id,
                    &source.full_name,
                    &reference,
                )
                .await?;
            }
            ScmProvider::Gitlab => {
                crate::integrations::gitlab::resolve_branch_commit(
                    &pool,
                    &state.encryption_key,
                    &source.integration_id,
                    &source.host_url,
                    &source.auth_mode,
                    &source.repository_external_id,
                    &reference,
                )
                .await?;
            }
            ScmProvider::LocalGit => {}
        }
    }

    Ok(Json(DiscoverRepositoryWorkflowsResponse {
        project_id,
        provider: source.provider,
        reference,
        workflows,
        truncated,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_omits_environment_values() {
        let preview = preview(
            ".oore.yaml".to_string(),
            "version: 1\nplatforms: [android]\nenv:\n  - key: API_TOKEN\n    value: super-secret\n",
        );
        let json = serde_json::to_string(&preview).expect("serialize preview");
        assert!(preview.valid);
        assert!(json.contains("API_TOKEN"));
        assert!(!json.contains("super-secret"));
    }

    #[test]
    fn candidates_are_deduplicated_and_bounded() {
        let discovered = (0..32).map(|index| format!(".oore/{index}.yaml"));
        let (paths, truncated) =
            workflow_candidates(Some(".oore/custom.yaml"), discovered).expect("candidates");
        assert_eq!(paths.len(), MAX_WORKFLOW_FILES);
        assert_eq!(paths[0], ".oore/custom.yaml");
        assert!(truncated);
    }

    #[test]
    fn malformed_yaml_errors_do_not_echo_values() {
        let preview = preview(
            ".oore.yaml".to_string(),
            "version: 1\nplatforms: [android]\nenv: do-not-return-this\n",
        );
        let json = serde_json::to_string(&preview).expect("serialize preview");
        assert!(!preview.valid);
        assert!(!json.contains("do-not-return-this"));
        assert!(json.contains("YAML is invalid"));
    }

    #[test]
    fn provider_urls_encode_repository_and_workflow_paths() {
        let github = github_contents_url("org/mobile app", ".oore/release.yml", "feature/x");
        assert_eq!(
            github.as_str(),
            "https://api.github.com/repos/org/mobile%20app/contents/.oore/release.yml?ref=feature%2Fx"
        );

        for host in ["https://gitlab.com", "https://gitlab.internal.example"] {
            let gitlab = gitlab_api_url(
                host,
                &[
                    "projects",
                    "group/mobile-app",
                    "repository",
                    "files",
                    ".oore/release.yml",
                    "raw",
                ],
            )
            .expect("GitLab URL");
            assert!(gitlab.as_str().contains("group%2Fmobile-app"));
            assert!(gitlab.as_str().contains(".oore%2Frelease.yml"));
        }
    }
}
