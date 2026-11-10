//! The operator side of testing: stable "latest" install links, tester
//! groups a build is sent to, who installed what, recent tester feedback,
//! and keeping a build from retention.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use oore_contract::{
    ApiError, ArtifactInstall, ArtifactInstallPlatform, BuildPlatform, BuildShare,
    BuildShareRecipient, CreateLatestInstallLinkRequest, LatestInstallLink,
    ListArtifactInstallsResponse, ListBuildSharesResponse, ListLatestInstallLinksResponse,
    ListRecentFeedbackResponse, ListTesterGroupsResponse, RecentFeedback, SendBuildToGroupRequest,
    TesterGroup, TesterGroupMember, TesterGroupRequest,
};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use tracing::{error, warn};
use uuid::Uuid;

use crate::AppState;
use crate::artifact_tokens::ValidatedDownloadToken;
use crate::extractors::AuthUser;
use crate::personal::{html_escape, queue_email, visible_projects};
use crate::project_rbac::{
    ProjectPermission, require_pipeline_project_permission, require_project_permission,
    resolve_effective_project_role,
};
use crate::store::write_audit_log;
use crate::token::{generate_token, hash_token};
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;
type ApiErr = (StatusCode, Json<ApiError>);

const MAX_GROUP_NAME: usize = 80;
const MAX_SHARE_MESSAGE: usize = 1000;
const MAX_BRANCH: usize = 255;

fn store_error(message: &'static str) -> impl FnOnce(sqlx::Error) -> ApiErr {
    move |e| {
        error!(error = %e, "{message}");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", message)
    }
}

fn is_admin(role: &str) -> bool {
    role == "owner" || role == "admin"
}

async fn require_build_permission(
    state: &AppState,
    auth: &AuthUser,
    build_id: &str,
    permission: ProjectPermission,
) -> Result<String, ApiErr> {
    let project_id: String = sqlx::query_scalar("SELECT project_id FROM builds WHERE id = ?1")
        .bind(build_id)
        .fetch_optional(&state.db)
        .await
        .map_err(store_error("Failed to load the build"))?
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Build not found"))?;
    let effective = resolve_effective_project_role(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, permission)?;
    Ok(project_id)
}

// ── Installs ────────────────────────────────────────────────────

/// Record an install through a download token, once per token. Install page
/// tokens belong to the person installing; a share or latest link could be
/// anyone, so those installs have no person.
pub async fn record_install(pool: &SqlitePool, token: &ValidatedDownloadToken) {
    let user_id = (token.purpose == "install_page").then_some(token.created_by.as_str());
    let result = sqlx::query(
        "INSERT OR IGNORE INTO artifact_installs \
         (id, artifact_id, build_id, user_id, via, token_hash, installed_at) \
         SELECT ?1, a.id, a.build_id, ?2, ?3, ?4, ?5 FROM artifacts a WHERE a.id = ?6",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(&token.purpose)
    .bind(&token.token_hash)
    .bind(now_unix())
    .bind(&token.artifact_id)
    .execute(pool)
    .await;
    if let Err(e) = result {
        warn!(error = %e, artifact_id = %token.artifact_id, "failed to record an install");
    }
}

/// `GET /v1/builds/{build_id}/installs` — who installed this build's files.
pub async fn list_build_installs(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<ListArtifactInstallsResponse> {
    require_build_permission(&state, &auth, &build_id, ProjectPermission::ReadArtifacts).await?;
    let rows = sqlx::query(
        "SELECT i.artifact_id, i.via, i.installed_at, u.email \
         FROM artifact_installs i LEFT JOIN users u ON u.id = i.user_id \
         WHERE i.build_id = ?1 ORDER BY i.installed_at DESC LIMIT 200",
    )
    .bind(&build_id)
    .fetch_all(&state.db)
    .await
    .map_err(store_error("Failed to load installs"))?;
    Ok(Json(ListArtifactInstallsResponse {
        installs: rows
            .iter()
            .map(|row| ArtifactInstall {
                artifact_id: row.get("artifact_id"),
                user_email: row.get("email"),
                via: row.get("via"),
                installed_at: row.get("installed_at"),
            })
            .collect(),
    }))
}

// ── Latest links ────────────────────────────────────────────────

fn platform_str(platform: BuildPlatform) -> Result<&'static str, ApiErr> {
    match platform {
        BuildPlatform::Android => Ok("android"),
        BuildPlatform::Ios => Ok("ios"),
        BuildPlatform::Macos => Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Latest links install on a phone: choose Android or iOS",
        )),
    }
}

fn parse_platform(value: &str) -> BuildPlatform {
    if value == "ios" {
        BuildPlatform::Ios
    } else {
        BuildPlatform::Android
    }
}

async fn public_base(pool: &SqlitePool) -> String {
    let settings = crate::instance_settings::load_effective_external_access_network_settings(pool)
        .await
        .ok();
    settings
        .and_then(|settings| settings.artifact_delivery_url.or(settings.public_url))
        .unwrap_or_else(|| "http://127.0.0.1:8787".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn link_from_row(row: &sqlx::sqlite::SqliteRow, url: Option<String>) -> LatestInstallLink {
    LatestInstallLink {
        id: row.get("id"),
        project_id: row.get("project_id"),
        pipeline_id: row.get("pipeline_id"),
        branch: row.get("branch"),
        platform: parse_platform(&row.get::<String, _>("platform")),
        url,
        prefix: row.get("token_prefix"),
        created_at: row.get("created_at"),
        created_by_email: row.get("email"),
        last_used_at: row.get("last_used_at"),
        use_count: row.get("use_count"),
    }
}

const LINK_COLUMNS: &str = "l.id, l.project_id, l.pipeline_id, l.branch, l.platform, \
     l.token_prefix, l.created_at, l.last_used_at, l.use_count, u.email";

/// `GET /v1/pipelines/{pipeline_id}/latest-links`
pub async fn list_latest_links(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
) -> ApiResult<ListLatestInstallLinksResponse> {
    require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::ReadArtifacts,
    )
    .await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {LINK_COLUMNS} FROM latest_install_links l \
         LEFT JOIN users u ON u.id = l.created_by \
         WHERE l.pipeline_id = ?1 ORDER BY l.created_at DESC"
    )))
    .bind(&pipeline_id)
    .fetch_all(&state.db)
    .await
    .map_err(store_error("Failed to load latest links"))?;
    Ok(Json(ListLatestInstallLinksResponse {
        links: rows.iter().map(|row| link_from_row(row, None)).collect(),
    }))
}

/// `POST /v1/pipelines/{pipeline_id}/latest-links`
pub async fn create_latest_link(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(pipeline_id): Path<String>,
    Json(req): Json<CreateLatestInstallLinkRequest>,
) -> ApiResult<LatestInstallLink> {
    let project_id = require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::WriteArtifacts,
    )
    .await?;
    let platform = platform_str(req.platform)?;
    let branch = req
        .branch
        .map(|branch| branch.trim().to_string())
        .filter(|branch| !branch.is_empty());
    if branch
        .as_ref()
        .is_some_and(|branch| branch.len() > MAX_BRANCH)
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "branch must be at most 255 characters",
        ));
    }
    let id = Uuid::new_v4().to_string();
    let token = generate_token();
    let now = now_unix();
    sqlx::query(
        "INSERT INTO latest_install_links \
         (id, project_id, pipeline_id, branch, platform, token_hash, token_prefix, created_by, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&pipeline_id)
    .bind(&branch)
    .bind(platform)
    .bind(hash_token(&token))
    .bind(&token[..8])
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(store_error("Failed to create the latest link"))?;
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "latest_install_link_created",
        "pipeline",
        Some(&pipeline_id),
        Some(
            &serde_json::json!({ "link_id": id, "platform": platform, "branch": branch })
                .to_string(),
        ),
    )
    .await;
    let url = format!("{}/install/latest/{token}", public_base(&state.db).await);
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {LINK_COLUMNS} FROM latest_install_links l \
         LEFT JOIN users u ON u.id = l.created_by WHERE l.id = ?1"
    )))
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .map_err(store_error("Failed to load the latest link"))?;
    Ok(Json(link_from_row(&row, Some(url))))
}

/// `DELETE /v1/latest-links/{link_id}`
pub async fn delete_latest_link(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(link_id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    let pipeline_id: String =
        sqlx::query_scalar("SELECT pipeline_id FROM latest_install_links WHERE id = ?1")
            .bind(&link_id)
            .fetch_optional(&state.db)
            .await
            .map_err(store_error("Failed to load the latest link"))?
            .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Link not found"))?;
    require_pipeline_project_permission(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        &auth.0.auth_source,
        &pipeline_id,
        ProjectPermission::WriteArtifacts,
    )
    .await?;
    sqlx::query("DELETE FROM latest_install_links WHERE id = ?1")
        .bind(&link_id)
        .execute(&state.db)
        .await
        .map_err(store_error("Failed to delete the latest link"))?;
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "latest_install_link_deleted",
        "pipeline",
        Some(&pipeline_id),
        Some(&serde_json::json!({ "link_id": link_id }).to_string()),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// The newest installable file of a succeeded build for a latest link.
async fn newest_artifact(
    pool: &SqlitePool,
    pipeline_id: &str,
    branch: Option<&str>,
    platform: &str,
) -> Result<Option<(String, i64, Option<String>, String)>, sqlx::Error> {
    let artifact_type = if platform == "ios" { "ipa" } else { "apk" };
    let row = sqlx::query(
        "SELECT a.id, b.build_number, b.version_name, p.name AS project_name \
         FROM artifacts a JOIN builds b ON b.id = a.build_id \
         JOIN projects p ON p.id = b.project_id \
         WHERE b.pipeline_id = ?1 AND b.status = 'succeeded' \
           AND (?2 IS NULL OR b.branch = ?2) \
           AND a.state = 'available' AND a.artifact_type = ?3 \
           AND (a.expires_at IS NULL OR a.expires_at > ?4) \
         ORDER BY b.created_at DESC, a.created_at DESC LIMIT 1",
    )
    .bind(pipeline_id)
    .bind(branch)
    .bind(artifact_type)
    .bind(now_unix())
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| {
        (
            row.get("id"),
            row.get("build_number"),
            row.get("version_name"),
            row.get("project_name"),
        )
    }))
}

fn install_page(title: &str, detail: &str, button: Option<(&str, &str)>) -> Response {
    let button = button
        .map(|(href, label)| {
            format!(
                "<a href=\"{}\" style=\"display:inline-block;margin-top:20px;padding:12px 22px;\
                 border-radius:999px;background:#c2410c;color:#fff;text-decoration:none;\
                 font-weight:600\">{}</a>",
                html_escape(href),
                html_escape(label)
            )
        })
        .unwrap_or_default();
    let body = format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\
         <meta name=\"robots\" content=\"noindex\"><title>{title}</title></head>\
         <body style=\"font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;\
         margin:0;padding:48px 24px;text-align:center;color:#1a1a1a\">\
         <h1 style=\"font-size:22px;margin:0 0 8px\">{title}</h1>\
         <p style=\"margin:0;color:#555\">{detail}</p>{button}</body></html>",
        title = html_escape(title),
        detail = html_escape(detail),
    );
    let mut response = Html(body).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

/// `GET /install/latest/{token}` — a page that installs the newest
/// succeeded build of the link's pipeline. Public: the token is the access.
pub async fn open_latest_link(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Response {
    let pool = &state.db;
    let row = match sqlx::query(
        "SELECT id, pipeline_id, branch, platform, created_by FROM latest_install_links \
         WHERE token_hash = ?1",
    )
    .bind(hash_token(&token))
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return install_page(
                "Link not found",
                "This install link was removed. Ask for a new one.",
                None,
            );
        }
        Err(e) => {
            error!(error = %e, "failed to load latest link");
            return install_page("Something went wrong", "Try again in a minute.", None);
        }
    };
    let link_id: String = row.get("id");
    let pipeline_id: String = row.get("pipeline_id");
    let branch: Option<String> = row.get("branch");
    let platform: String = row.get("platform");
    let created_by: String = row.get("created_by");
    let artifact = match newest_artifact(pool, &pipeline_id, branch.as_deref(), &platform).await {
        Ok(Some(artifact)) => artifact,
        Ok(None) => {
            return install_page(
                "No build to install yet",
                "No succeeded build of this pipeline has an installable file right now.",
                None,
            );
        }
        Err(e) => {
            error!(error = %e, "failed to find the newest build for a latest link");
            return install_page("Something went wrong", "Try again in a minute.", None);
        }
    };
    let (artifact_id, build_number, version, project_name) = artifact;
    let link = match crate::artifact_install::install_link_for_artifact(
        &state,
        &artifact_id,
        &created_by,
        "latest_link",
    )
    .await
    {
        Ok(link) => link,
        Err((_, Json(e))) => {
            return install_page("This build cannot be installed", &e.error, None);
        }
    };
    let _ = sqlx::query(
        "UPDATE latest_install_links SET use_count = use_count + 1, last_used_at = ?1 WHERE id = ?2",
    )
    .bind(now_unix())
    .bind(&link_id)
    .execute(pool)
    .await;
    let title = match version {
        Some(version) => format!("{project_name} {version}"),
        None => project_name,
    };
    let detail = format!(
        "Build #{build_number}{}. The newest build of this link, every time you open it.",
        branch
            .as_deref()
            .map(|branch| format!(" from {branch}"))
            .unwrap_or_default()
    );
    let label = match link.platform {
        ArtifactInstallPlatform::Ios => "Install on this iPhone",
        ArtifactInstallPlatform::Android => "Download the APK",
    };
    install_page(&title, &detail, Some((&link.install_url, label)))
}

// ── Tester groups ───────────────────────────────────────────────

async fn load_groups(
    pool: &SqlitePool,
    group_id: Option<&str>,
) -> Result<Vec<TesterGroup>, sqlx::Error> {
    let groups = sqlx::query(
        "SELECT id, name, created_at, updated_at FROM tester_groups \
         WHERE ?1 IS NULL OR id = ?1 ORDER BY name COLLATE NOCASE",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?;
    let members = sqlx::query(
        "SELECT m.group_id, u.id, u.email, u.display_name FROM tester_group_members m \
         JOIN users u ON u.id = m.user_id WHERE ?1 IS NULL OR m.group_id = ?1 \
         ORDER BY u.email",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?;
    let mut by_group: HashMap<String, Vec<TesterGroupMember>> = HashMap::new();
    for row in &members {
        by_group
            .entry(row.get("group_id"))
            .or_default()
            .push(TesterGroupMember {
                user_id: row.get("id"),
                email: row.get("email"),
                name: row.get("display_name"),
            });
    }
    Ok(groups
        .iter()
        .map(|row| {
            let id: String = row.get("id");
            TesterGroup {
                members: by_group.remove(&id).unwrap_or_default(),
                id,
                name: row.get("name"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            }
        })
        .collect())
}

fn require_not_tester(auth: &AuthUser) -> Result<(), ApiErr> {
    if auth.0.role == "qa_viewer" {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "insufficient_role",
            "You do not have permission to perform this action",
        ));
    }
    Ok(())
}

/// `GET /v1/tester-groups`
pub async fn list_tester_groups(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<ListTesterGroupsResponse> {
    require_not_tester(&auth)?;
    Ok(Json(ListTesterGroupsResponse {
        groups: load_groups(&state.db, None)
            .await
            .map_err(store_error("Failed to load tester groups"))?,
    }))
}

fn validate_group(req: &TesterGroupRequest) -> Result<String, ApiErr> {
    let name = req.name.trim().to_string();
    if name.is_empty() || name.chars().count() > MAX_GROUP_NAME {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "name must be 1 to 80 characters",
        ));
    }
    Ok(name)
}

async fn save_group(
    state: &AppState,
    auth: &AuthUser,
    group_id: &str,
    name: &str,
    member_user_ids: &[String],
    create: bool,
) -> Result<(), ApiErr> {
    let now = now_unix();
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(store_error("Failed to save the tester group"))?;
    let saved = if create {
        sqlx::query(
            "INSERT INTO tester_groups (id, name, created_by, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?4)",
        )
        .bind(group_id)
        .bind(name)
        .bind(&auth.0.user_id)
        .bind(now)
        .execute(&mut *tx)
        .await
    } else {
        sqlx::query("UPDATE tester_groups SET name = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(name)
            .bind(now)
            .bind(group_id)
            .execute(&mut *tx)
            .await
    };
    match saved {
        Ok(result) if result.rows_affected() == 0 => {
            return Err(api_err(
                StatusCode::NOT_FOUND,
                "not_found",
                "Tester group not found",
            ));
        }
        Ok(_) => {}
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(api_err(
                StatusCode::CONFLICT,
                "name_taken",
                "A tester group with this name already exists",
            ));
        }
        Err(e) => return Err(store_error("Failed to save the tester group")(e)),
    }
    sqlx::query("DELETE FROM tester_group_members WHERE group_id = ?1")
        .bind(group_id)
        .execute(&mut *tx)
        .await
        .map_err(store_error("Failed to save the tester group"))?;
    for user_id in member_user_ids {
        sqlx::query(
            "INSERT OR IGNORE INTO tester_group_members (group_id, user_id) \
             SELECT ?1, id FROM users WHERE id = ?2",
        )
        .bind(group_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(store_error("Failed to save the tester group"))?;
    }
    tx.commit()
        .await
        .map_err(store_error("Failed to save the tester group"))?;
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        if create {
            "tester_group_created"
        } else {
            "tester_group_updated"
        },
        "tester_group",
        Some(group_id),
        Some(&serde_json::json!({ "name": name, "members": member_user_ids.len() }).to_string()),
    )
    .await;
    Ok(())
}

async fn one_group(pool: &SqlitePool, group_id: &str) -> ApiResult<TesterGroup> {
    load_groups(pool, Some(group_id))
        .await
        .map_err(store_error("Failed to load the tester group"))?
        .into_iter()
        .next()
        .map(Json)
        .ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "Tester group not found"))
}

/// `POST /v1/tester-groups` — owners and admins.
pub async fn create_tester_group(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<TesterGroupRequest>,
) -> ApiResult<TesterGroup> {
    auth.require_role(&["owner", "admin"])?;
    let name = validate_group(&req)?;
    let id = Uuid::new_v4().to_string();
    save_group(&state, &auth, &id, &name, &req.member_user_ids, true).await?;
    one_group(&state.db, &id).await
}

/// `PUT /v1/tester-groups/{group_id}` — owners and admins.
pub async fn update_tester_group(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(group_id): Path<String>,
    Json(req): Json<TesterGroupRequest>,
) -> ApiResult<TesterGroup> {
    auth.require_role(&["owner", "admin"])?;
    let name = validate_group(&req)?;
    save_group(&state, &auth, &group_id, &name, &req.member_user_ids, false).await?;
    one_group(&state.db, &group_id).await
}

/// `DELETE /v1/tester-groups/{group_id}` — owners and admins.
pub async fn delete_tester_group(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(group_id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    auth.require_role(&["owner", "admin"])?;
    let result = sqlx::query("DELETE FROM tester_groups WHERE id = ?1")
        .bind(&group_id)
        .execute(&state.db)
        .await
        .map_err(store_error("Failed to delete the tester group"))?;
    if result.rows_affected() == 0 {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Tester group not found",
        ));
    }
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        "tester_group_deleted",
        "tester_group",
        Some(&group_id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ── Sending a build to a group ──────────────────────────────────

async fn load_shares(pool: &SqlitePool, build_id: &str) -> Result<Vec<BuildShare>, sqlx::Error> {
    let shares = sqlx::query(
        "SELECT s.id, s.build_id, s.group_id, g.name AS group_name, s.message, s.sent_at, \
                u.email AS sent_by_email \
         FROM build_shares s JOIN tester_groups g ON g.id = s.group_id \
         LEFT JOIN users u ON u.id = s.sent_by \
         WHERE s.build_id = ?1 ORDER BY s.sent_at DESC",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?;
    let mut result = Vec::new();
    for share in shares {
        let share_id: String = share.get("id");
        let recipients = sqlx::query(
            "SELECT r.user_id, r.no_access, u.email, u.display_name, \
                    (SELECT MIN(i.installed_at) FROM artifact_installs i \
                     WHERE i.build_id = ?2 AND i.user_id = r.user_id) AS installed_at \
             FROM build_share_recipients r JOIN users u ON u.id = r.user_id \
             WHERE r.share_id = ?1 ORDER BY u.email",
        )
        .bind(&share_id)
        .bind(build_id)
        .fetch_all(pool)
        .await?;
        result.push(BuildShare {
            id: share_id,
            build_id: share.get("build_id"),
            group_id: share.get("group_id"),
            group_name: share.get("group_name"),
            message: share.get("message"),
            sent_by_email: share.get("sent_by_email"),
            sent_at: share.get("sent_at"),
            recipients: recipients
                .iter()
                .map(|row| BuildShareRecipient {
                    user_id: row.get("user_id"),
                    email: row.get("email"),
                    name: row.get("display_name"),
                    installed_at: row.get("installed_at"),
                    no_access: row.get::<i64, _>("no_access") != 0,
                })
                .collect(),
        });
    }
    Ok(result)
}

/// `GET /v1/builds/{build_id}/shares`
pub async fn list_build_shares(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> ApiResult<ListBuildSharesResponse> {
    require_build_permission(&state, &auth, &build_id, ProjectPermission::ReadArtifacts).await?;
    Ok(Json(ListBuildSharesResponse {
        shares: load_shares(&state.db, &build_id)
            .await
            .map_err(store_error("Failed to load who the build was sent to"))?,
    }))
}

fn share_email(
    project_name: &str,
    build_number: i64,
    version: Option<&str>,
    sender: &str,
    message: Option<&str>,
    link: Option<&str>,
) -> (String, String) {
    let what = match version {
        Some(version) => format!("{project_name} {version} (build #{build_number})"),
        None => format!("{project_name} build #{build_number}"),
    };
    let subject = format!("{what} is ready to test");
    let mut body = String::from(
        "<div style=\"font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;\
         max-width:560px;margin:0 auto;color:#1a1a1a;line-height:1.5\">",
    );
    body.push_str(&format!(
        "<h2 style=\"font-size:18px;margin:0 0 4px\">{}</h2>\
         <p style=\"margin:0 0 16px;color:#555\">{} sent it to you to test.</p>",
        html_escape(&subject),
        html_escape(sender)
    ));
    if let Some(message) = message {
        body.push_str(&format!(
            "<p style=\"margin:0 0 16px;padding:12px;background:#f5f5f4;border-radius:8px\">{}</p>",
            html_escape(message)
        ));
    }
    if let Some(link) = link {
        body.push_str(&format!(
            "<p><a href=\"{}\" style=\"display:inline-block;padding:10px 18px;border-radius:999px;\
             background:#c2410c;color:#fff;text-decoration:none;font-weight:600\">Open and install</a></p>",
            html_escape(link)
        ));
    }
    body.push_str("</div>");
    (subject, body)
}

/// `POST /v1/builds/{build_id}/shares` — send a build to a tester group:
/// everyone in it who can open the app gets an email and a push.
pub async fn send_build_to_group(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
    Json(req): Json<SendBuildToGroupRequest>,
) -> ApiResult<BuildShare> {
    let project_id =
        require_build_permission(&state, &auth, &build_id, ProjectPermission::WriteArtifacts)
            .await?;
    let message = req
        .message
        .map(|message| message.trim().to_string())
        .filter(|message| !message.is_empty());
    if message
        .as_ref()
        .is_some_and(|message| message.chars().count() > MAX_SHARE_MESSAGE)
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "message must be at most 1000 characters",
        ));
    }
    let pool = &state.db;
    let build = sqlx::query(
        "SELECT b.build_number, b.version_name, b.status, p.name AS project_name \
         FROM builds b JOIN projects p ON p.id = b.project_id WHERE b.id = ?1",
    )
    .bind(&build_id)
    .fetch_one(pool)
    .await
    .map_err(store_error("Failed to load the build"))?;
    if build.get::<String, _>("status") != "succeeded" {
        return Err(api_err(
            StatusCode::CONFLICT,
            "build_not_succeeded",
            "Only a succeeded build can be sent to testers",
        ));
    }
    let members = sqlx::query(
        "SELECT u.id, u.role FROM tester_group_members m JOIN users u ON u.id = m.user_id \
         WHERE m.group_id = ?1 AND u.status = 'active'",
    )
    .bind(&req.group_id)
    .fetch_all(pool)
    .await
    .map_err(store_error("Failed to load the tester group"))?;
    let group_exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM tester_groups WHERE id = ?1)")
            .bind(&req.group_id)
            .fetch_one(pool)
            .await
            .map_err(store_error("Failed to load the tester group"))?;
    if !group_exists {
        return Err(api_err(
            StatusCode::NOT_FOUND,
            "not_found",
            "Tester group not found",
        ));
    }

    let build_number: i64 = build.get("build_number");
    let version: Option<String> = build.get("version_name");
    let project_name: String = build.get("project_name");
    let link = crate::commit_checks::frontend_base_url(pool)
        .await
        .map(|base| format!("{}/builds/{build_id}", base.as_str().trim_end_matches('/')));
    let (subject, html) = share_email(
        &project_name,
        build_number,
        version.as_deref(),
        &auth.0.email,
        message.as_deref(),
        link.as_deref(),
    );

    let share_id = Uuid::new_v4().to_string();
    let now = now_unix();
    let mut tx = pool
        .begin()
        .await
        .map_err(store_error("Failed to send the build"))?;
    sqlx::query(
        "INSERT INTO build_shares (id, build_id, group_id, message, sent_by, sent_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&share_id)
    .bind(&build_id)
    .bind(&req.group_id)
    .bind(&message)
    .bind(&auth.0.user_id)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(store_error("Failed to send the build"))?;
    let mut reached = Vec::new();
    for member in &members {
        let user_id: String = member.get("id");
        let role: String = member.get("role");
        let has_access = is_admin(&role)
            || sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM project_members WHERE user_id = ?1 AND project_id = ?2",
            )
            .bind(&user_id)
            .bind(&project_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(store_error("Failed to send the build"))?
                > 0;
        sqlx::query(
            "INSERT INTO build_share_recipients (share_id, user_id, no_access) VALUES (?1, ?2, ?3)",
        )
        .bind(&share_id)
        .bind(&user_id)
        .bind(!has_access)
        .execute(&mut *tx)
        .await
        .map_err(store_error("Failed to send the build"))?;
        if has_access {
            queue_email(&mut tx, &user_id, &subject, &html)
                .await
                .map_err(store_error("Failed to send the build"))?;
            reached.push(user_id);
        }
    }
    tx.commit()
        .await
        .map_err(store_error("Failed to send the build"))?;

    crate::web_push::push_to_users(
        &state,
        &reached,
        &crate::web_push::PushMessage {
            title: subject.clone(),
            body: message
                .clone()
                .unwrap_or_else(|| format!("Sent by {}", auth.0.email)),
            url: format!("/builds/{build_id}"),
        },
    );
    let _ = write_audit_log(
        pool,
        Some(&auth.0.user_id),
        "build_sent_to_testers",
        "build",
        Some(&build_id),
        Some(
            &serde_json::json!({ "group_id": req.group_id, "recipients": reached.len() })
                .to_string(),
        ),
    )
    .await;
    load_shares(pool, &build_id)
        .await
        .map_err(store_error("Failed to load who the build was sent to"))?
        .into_iter()
        .find(|share| share.id == share_id)
        .map(Json)
        .ok_or_else(|| {
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to load the share",
            )
        })
}

// ── Recent feedback ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RecentFeedbackQuery {
    pub days: Option<i64>,
}

/// `GET /v1/feedback/recent` — tester feedback from the last `days` (1 to
/// 30, default 7) on apps the caller can see, newest first.
pub async fn list_recent_feedback(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(query): Query<RecentFeedbackQuery>,
) -> ApiResult<ListRecentFeedbackResponse> {
    require_not_tester(&auth)?;
    let days = query.days.unwrap_or(7).clamp(1, 30);
    let since = now_unix() - days * 86_400;
    let projects: HashMap<String, String> =
        visible_projects(&state.db, &auth.0.user_id, &auth.0.role)
            .await
            .map_err(store_error("Failed to load apps"))?
            .into_iter()
            .collect();
    let rows = sqlx::query(
        "SELECT f.id, b.project_id, b.build_number FROM build_feedback f \
         JOIN builds b ON b.id = f.build_id WHERE f.created_at >= ?1 \
         ORDER BY f.created_at DESC LIMIT 100",
    )
    .bind(since)
    .fetch_all(&state.db)
    .await
    .map_err(store_error("Failed to load feedback"))?;
    let mut feedback = Vec::new();
    for row in rows {
        let project_id: String = row.get("project_id");
        let Some(project_name) = projects.get(&project_id) else {
            continue;
        };
        let id: String = row.get("id");
        if let Some(item) = crate::build_feedback::load_feedback(&state.db, &id)
            .await
            .map_err(store_error("Failed to load feedback"))?
        {
            feedback.push(RecentFeedback {
                feedback: item,
                project_id,
                project_name: project_name.clone(),
                build_number: row.get("build_number"),
            });
        }
    }
    Ok(Json(ListRecentFeedbackResponse { feedback }))
}

// ── Keeping a build ─────────────────────────────────────────────

/// `PUT /v1/builds/{build_id}/keep` — retention never removes this build.
pub async fn keep_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    set_kept(&state, &auth, &build_id, true).await
}

/// `DELETE /v1/builds/{build_id}/keep` — retention applies again.
pub async fn unkeep_build(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(build_id): Path<String>,
) -> Result<StatusCode, ApiErr> {
    set_kept(&state, &auth, &build_id, false).await
}

async fn set_kept(
    state: &AppState,
    auth: &AuthUser,
    build_id: &str,
    keep: bool,
) -> Result<StatusCode, ApiErr> {
    require_build_permission(state, auth, build_id, ProjectPermission::WriteArtifacts).await?;
    let now = now_unix();
    sqlx::query(
        "UPDATE builds SET kept_at = CASE WHEN ?1 THEN COALESCE(kept_at, ?2) END, \
         kept_by = CASE WHEN ?1 THEN COALESCE(kept_by, ?3) END WHERE id = ?4",
    )
    .bind(keep)
    .bind(now)
    .bind(&auth.0.user_id)
    .bind(build_id)
    .execute(&state.db)
    .await
    .map_err(store_error("Failed to update the build"))?;
    let _ = write_audit_log(
        &state.db,
        Some(&auth.0.user_id),
        if keep { "build_kept" } else { "build_unkept" },
        "build",
        Some(build_id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_email_escapes_and_links() {
        let (subject, html) = share_email(
            "Shop",
            12,
            Some("2.4.0"),
            "lead@example.com",
            Some("Check <checkout>"),
            Some("https://oore.example.com/builds/b1"),
        );
        assert_eq!(subject, "Shop 2.4.0 (build #12) is ready to test");
        assert!(html.contains("Check &lt;checkout&gt;"));
        assert!(html.contains("https://oore.example.com/builds/b1"));
    }

    #[test]
    fn latest_links_are_for_phones() {
        assert_eq!(platform_str(BuildPlatform::Ios).unwrap(), "ios");
        assert!(platform_str(BuildPlatform::Macos).is_err());
    }
}
