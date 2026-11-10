//! Things that belong to one person rather than the instance: pinned apps,
//! notification choices, the inbox behind the bell, personal email and the
//! weekly digest.
//!
//! Personal email goes through the SMTP server of the instance's first
//! enabled email channel, so nothing new needs configuring; without one the
//! web UI says email is unavailable and only browser notifications work.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, BuildPlatform, DigestApp, DigestExpiry, DigestPipeline, DigestRelease, InboxItem,
    InboxItemKind, InboxResponse, NotificationPreferences, NotificationPreferencesResponse,
    PersonalBuildEvents, PipelineExecutionConfig, Project, SmtpConfig, WeeklyDigest,
};
use sqlx::{Row, SqliteConnection, SqlitePool};
use tracing::{error, warn};

use crate::AppState;
use crate::crypto;
use crate::extractors::AuthUser;
use crate::project_rbac::{
    ProjectPermission, require_project_permission, resolve_effective_project_role,
};
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const DAY: i64 = 86_400;
const INBOX_WINDOW: i64 = 7 * DAY;
const INBOX_LIMIT: usize = 50;
const EXPIRY_WINDOW: i64 = 30 * DAY;
const MAILER_INTERVAL: Duration = Duration::from_secs(30);
const EMAIL_MAX_ATTEMPTS: i64 = 5;
/// The digest goes out on Monday from this hour (UTC).
const DIGEST_HOUR_UTC: i64 = 8;

fn store_error(message: &'static str) -> impl FnOnce(sqlx::Error) -> (StatusCode, Json<ApiError>) {
    move |e| {
        error!(error = %e, "{message}");
        api_err(StatusCode::INTERNAL_SERVER_ERROR, "store_error", message)
    }
}

fn is_admin(role: &str) -> bool {
    role == "owner" || role == "admin"
}

/// Apps this person can see: every app for owners and admins, otherwise the
/// ones they are a member of.
async fn visible_projects(
    pool: &SqlitePool,
    user_id: &str,
    role: &str,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows = if is_admin(role) {
        sqlx::query("SELECT id, name FROM projects ORDER BY name")
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query(
            "SELECT p.id, p.name FROM projects p \
             JOIN project_members pm ON pm.project_id = p.id AND pm.user_id = ?1 \
             ORDER BY p.name",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?
    };
    Ok(rows
        .iter()
        .map(|row| (row.get("id"), row.get("name")))
        .collect())
}

/// Apps where this person can approve a gated build.
async fn approvable_projects(
    pool: &SqlitePool,
    user_id: &str,
    role: &str,
) -> Result<HashSet<String>, sqlx::Error> {
    if is_admin(role) {
        return Ok(sqlx::query_scalar("SELECT id FROM projects")
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect());
    }
    if role == "qa_viewer" {
        return Ok(HashSet::new());
    }
    Ok(sqlx::query_scalar(
        "SELECT project_id FROM project_members WHERE user_id = ?1 AND role = 'maintainer'",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect())
}

// ── Pinned apps and list decoration ─────────────────────────────

/// Fills the per-person and activity fields of listed apps.
pub async fn decorate_projects(
    pool: &SqlitePool,
    user_id: &str,
    projects: &mut [Project],
) -> Result<(), sqlx::Error> {
    if projects.is_empty() {
        return Ok(());
    }
    let ids: Vec<&str> = projects.iter().map(|p| p.id.as_str()).collect();
    let placeholders = vec!["?"; ids.len()].join(", ");

    let favorites: HashSet<String> =
        sqlx::query_scalar("SELECT project_id FROM user_favorite_projects WHERE user_id = ?1")
            .bind(user_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();

    let sql = format!(
        "SELECT project_id, MAX(created_at) AS last FROM builds \
         WHERE project_id IN ({placeholders}) GROUP BY project_id"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
    for id in &ids {
        query = query.bind(*id);
    }
    let last_builds: HashMap<String, i64> = query
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| (row.get("project_id"), row.get("last")))
        .collect();

    let sql = format!(
        "SELECT project_id, execution_config FROM pipelines WHERE project_id IN ({placeholders})"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
    for id in &ids {
        query = query.bind(*id);
    }
    let mut platforms: HashMap<String, Vec<BuildPlatform>> = HashMap::new();
    for row in query.fetch_all(pool).await? {
        let project_id: String = row.get("project_id");
        let config: PipelineExecutionConfig =
            serde_json::from_str(&row.get::<String, _>("execution_config")).unwrap_or_default();
        let entry = platforms.entry(project_id).or_default();
        for platform in config.platforms {
            if !entry.contains(&platform) {
                entry.push(platform);
            }
        }
    }

    let sql = format!(
        "SELECT project_id, version_name FROM releases \
         WHERE project_id IN ({placeholders}) AND state IN ('rolling', 'complete') \
           AND version_name IS NOT NULL \
         ORDER BY updated_at DESC"
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
    for id in &ids {
        query = query.bind(*id);
    }
    let mut live: HashMap<String, String> = HashMap::new();
    for row in query.fetch_all(pool).await? {
        live.entry(row.get("project_id"))
            .or_insert_with(|| row.get("version_name"));
    }

    for project in projects.iter_mut() {
        project.is_favorite = favorites.contains(&project.id);
        project.last_build_at = last_builds.get(&project.id).copied();
        let mut found = platforms.remove(&project.id).unwrap_or_default();
        found.sort_by_key(|p| match p {
            BuildPlatform::Android => 0,
            BuildPlatform::Ios => 1,
            BuildPlatform::Macos => 2,
        });
        project.platforms = found;
        project.live_version = live.remove(&project.id);
    }
    Ok(())
}

async fn require_readable(
    state: &AppState,
    auth: &AuthUser,
    project_id: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let effective = resolve_effective_project_role(
        &state.db,
        &auth.0.user_id,
        &auth.0.role,
        project_id,
        &auth.0.auth_source,
    )
    .await?;
    require_project_permission(&effective, ProjectPermission::Read)
}

/// `PUT /v1/projects/{project_id}/favorite` — pin an app for yourself.
pub async fn add_favorite(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    require_readable(&state, &auth, &project_id).await?;
    sqlx::query(
        "INSERT INTO user_favorite_projects (user_id, project_id, created_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT DO NOTHING",
    )
    .bind(&auth.0.user_id)
    .bind(&project_id)
    .bind(now_unix())
    .execute(&state.db)
    .await
    .map_err(store_error("Failed to pin the app"))?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /v1/projects/{project_id}/favorite` — unpin an app.
pub async fn remove_favorite(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(project_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    sqlx::query("DELETE FROM user_favorite_projects WHERE user_id = ?1 AND project_id = ?2")
        .bind(&auth.0.user_id)
        .bind(&project_id)
        .execute(&state.db)
        .await
        .map_err(store_error("Failed to unpin the app"))?;
    Ok(StatusCode::NO_CONTENT)
}

// ── Preferences ─────────────────────────────────────────────────

pub async fn load_preferences(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<NotificationPreferences, sqlx::Error> {
    let json: Option<String> = sqlx::query_scalar(
        "SELECT preferences_json FROM user_notification_preferences WHERE user_id = ?1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(json
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

/// The SMTP settings of the first enabled email channel, if any.
async fn instance_mailer(
    pool: &SqlitePool,
    encryption_key: &[u8],
) -> Result<Option<SmtpConfig>, sqlx::Error> {
    let encrypted: Option<String> = sqlx::query_scalar(
        "SELECT encrypted_config FROM notification_channels \
         WHERE channel_type = 'email' AND enabled = 1 AND encrypted_config IS NOT NULL \
         ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(encrypted.and_then(|encrypted| {
        crypto::decrypt(&encrypted, encryption_key)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
    }))
}

async fn email_available(pool: &SqlitePool) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM notification_channels \
         WHERE channel_type = 'email' AND enabled = 1 AND encrypted_config IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0)
        > 0
}

/// `GET /v1/users/me/notification-preferences`
pub async fn get_preferences(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<NotificationPreferencesResponse> {
    let preferences = load_preferences(&state.db, &auth.0.user_id)
        .await
        .map_err(store_error("Failed to load notification preferences"))?;
    Ok(Json(NotificationPreferencesResponse {
        preferences,
        email_available: email_available(&state.db).await,
        email_address: auth.0.email.clone(),
    }))
}

/// `PUT /v1/users/me/notification-preferences`
pub async fn update_preferences(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(preferences): Json<NotificationPreferences>,
) -> ApiResult<NotificationPreferencesResponse> {
    let json = serde_json::to_string(&preferences).map_err(|_| {
        api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "Invalid notification preferences",
        )
    })?;
    sqlx::query(
        "INSERT INTO user_notification_preferences (user_id, preferences_json, updated_at) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT (user_id) DO UPDATE SET preferences_json = ?2, updated_at = ?3",
    )
    .bind(&auth.0.user_id)
    .bind(&json)
    .bind(now_unix())
    .execute(&state.db)
    .await
    .map_err(store_error("Failed to save notification preferences"))?;
    Ok(Json(NotificationPreferencesResponse {
        preferences,
        email_available: email_available(&state.db).await,
        email_address: auth.0.email.clone(),
    }))
}

// ── Inbox ───────────────────────────────────────────────────────

/// "Google Play · production", "TestFlight" or "App Store".
fn release_place(store: &str, track: &str) -> String {
    match (store, track) {
        ("app_store", "testflight") => "TestFlight".to_string(),
        ("app_store", _) => "App Store".to_string(),
        _ => format!("{} · {track}", store_words(store)),
    }
}

fn release_state_words(state: &str) -> &str {
    match state {
        "blocked" => "was refused by the store",
        "in_review" => "is in review",
        "approved" => "was approved",
        "rejected" => "was rejected",
        "rolling" => "is rolling out",
        "paused" => "was paused",
        "halted" => "was halted",
        "complete" => "is live",
        "failed" => "failed",
        "processing" => "is processing",
        _ => "changed",
    }
}

fn store_words(store: &str) -> &str {
    match store {
        "app_store" => "App Store",
        _ => "Google Play",
    }
}

/// The inbox for one person, newest first.
pub async fn inbox_for(
    pool: &SqlitePool,
    user_id: &str,
    email: &str,
    role: &str,
    now: i64,
) -> Result<Vec<InboxItem>, sqlx::Error> {
    let since = now - INBOX_WINDOW;
    let names: HashMap<String, String> = visible_projects(pool, user_id, role)
        .await?
        .into_iter()
        .collect();
    let mut items = Vec::new();

    let approvable = approvable_projects(pool, user_id, role).await?;
    if !approvable.is_empty() {
        let rows = sqlx::query(
            "SELECT b.id, b.project_id, b.build_number, b.branch, b.approval_requested_at, \
                    b.created_at, pl.name AS pipeline_name \
             FROM builds b LEFT JOIN pipelines pl ON pl.id = b.pipeline_id \
             WHERE b.status = 'queued' AND b.approval_state = 'pending' \
             ORDER BY b.created_at DESC LIMIT 100",
        )
        .fetch_all(pool)
        .await?;
        for row in rows {
            let project_id: String = row.get("project_id");
            if !approvable.contains(&project_id) {
                continue;
            }
            let Some(project_name) = names.get(&project_id) else {
                continue;
            };
            let number: i64 = row.try_get("build_number").unwrap_or_default();
            let pipeline: Option<String> = row.get("pipeline_name");
            let branch: Option<String> = row.try_get("branch").ok().flatten();
            items.push(InboxItem {
                id: format!("approval:{}", row.get::<String, _>("id")),
                kind: InboxItemKind::Approval,
                title: format!("Build #{number} waits for your approval"),
                detail: [pipeline, branch]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · "),
                project_id,
                project_name: project_name.clone(),
                build_id: Some(row.get("id")),
                release_id: None,
                at: row
                    .get::<Option<i64>, _>("approval_requested_at")
                    .unwrap_or_else(|| row.get("created_at")),
            });
        }
    }

    let rows = sqlx::query(
        "SELECT b.id, b.project_id, b.build_number, b.status, b.branch, \
                COALESCE(b.finished_at, b.updated_at) AS at, pl.name AS pipeline_name \
         FROM builds b LEFT JOIN pipelines pl ON pl.id = b.pipeline_id \
         WHERE b.trigger_actor = ?1 AND b.status IN ('failed', 'timed_out') \
           AND COALESCE(b.finished_at, b.updated_at) >= ?2 \
         ORDER BY at DESC LIMIT 50",
    )
    .bind(email)
    .bind(since)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let project_id: String = row.get("project_id");
        let Some(project_name) = names.get(&project_id) else {
            continue;
        };
        let number: i64 = row.try_get("build_number").unwrap_or_default();
        let status: String = row.get("status");
        let pipeline: Option<String> = row.get("pipeline_name");
        let branch: Option<String> = row.try_get("branch").ok().flatten();
        items.push(InboxItem {
            id: format!("failed:{}", row.get::<String, _>("id")),
            kind: InboxItemKind::FailedBuild,
            title: format!(
                "Your build #{number} {}",
                if status == "timed_out" {
                    "timed out"
                } else {
                    "failed"
                }
            ),
            detail: [pipeline, branch]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
            project_id,
            project_name: project_name.clone(),
            build_id: Some(row.get("id")),
            release_id: None,
            at: row.get("at"),
        });
    }

    let rows = sqlx::query(
        "SELECT h.id, h.to_state, h.at, r.id AS release_id, r.project_id, r.store, r.track, \
                r.version_name, r.build_id \
         FROM release_history h \
         JOIN releases r ON r.id = h.release_id \
         JOIN user_favorite_projects f ON f.project_id = r.project_id AND f.user_id = ?1 \
         WHERE h.at >= ?2 AND h.to_state IN ('blocked', 'in_review', 'approved', 'rejected', \
               'rolling', 'paused', 'halted', 'complete', 'failed') \
         ORDER BY h.at DESC LIMIT 50",
    )
    .bind(user_id)
    .bind(since)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let project_id: String = row.get("project_id");
        let Some(project_name) = names.get(&project_id) else {
            continue;
        };
        let version: Option<String> = row.get("version_name");
        let state: String = row.get("to_state");
        let store: String = row.get("store");
        items.push(InboxItem {
            id: format!("release:{}", row.get::<String, _>("id")),
            kind: InboxItemKind::Release,
            title: format!(
                "{project_name} {} {}",
                version.as_deref().unwrap_or("release"),
                release_state_words(&state)
            ),
            detail: release_place(&store, &row.get::<String, _>("track")),
            project_id,
            project_name: project_name.clone(),
            build_id: row.get("build_id"),
            release_id: Some(row.get("release_id")),
            at: row.get("at"),
        });
    }

    items.sort_by_key(|item| std::cmp::Reverse(item.at));
    items.truncate(INBOX_LIMIT);
    Ok(items)
}

/// `GET /v1/users/me/inbox`
pub async fn get_inbox(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<InboxResponse> {
    let items = inbox_for(
        &state.db,
        &auth.0.user_id,
        &auth.0.email,
        &auth.0.role,
        now_unix(),
    )
    .await
    .map_err(store_error("Failed to load the inbox"))?;
    Ok(Json(InboxResponse { items }))
}

// ── Weekly digest ───────────────────────────────────────────────

fn median(values: &mut [i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[values.len() / 2])
}

/// The last 7 days of the apps this person pinned, or every app they can
/// see when they pinned none.
pub async fn digest_for(
    pool: &SqlitePool,
    user_id: &str,
    role: &str,
    now: i64,
) -> Result<WeeklyDigest, sqlx::Error> {
    let since = now - 7 * DAY;
    let visible = visible_projects(pool, user_id, role).await?;
    let pinned: HashSet<String> =
        sqlx::query_scalar("SELECT project_id FROM user_favorite_projects WHERE user_id = ?1")
            .bind(user_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();
    let projects: Vec<(String, String)> = if pinned.is_empty() {
        visible
    } else {
        visible
            .into_iter()
            .filter(|(id, _)| pinned.contains(id))
            .collect()
    };

    let mut apps = Vec::new();
    let mut expiring = Vec::new();
    for (project_id, name) in &projects {
        let rows = sqlx::query(
            "SELECT b.status, b.started_at, b.finished_at, pl.name AS pipeline_name \
             FROM builds b LEFT JOIN pipelines pl ON pl.id = b.pipeline_id \
             WHERE b.project_id = ?1 AND b.finished_at >= ?2",
        )
        .bind(project_id)
        .bind(since)
        .fetch_all(pool)
        .await?;
        let mut succeeded = 0;
        let mut failed = 0;
        let mut durations: HashMap<String, Vec<i64>> = HashMap::new();
        for row in &rows {
            let status: String = row.get("status");
            match status.as_str() {
                "succeeded" => succeeded += 1,
                "failed" | "timed_out" => failed += 1,
                _ => {}
            }
            if status == "succeeded"
                && let (Some(start), Some(end), Some(pipeline)) = (
                    row.get::<Option<i64>, _>("started_at"),
                    row.get::<Option<i64>, _>("finished_at"),
                    row.get::<Option<String>, _>("pipeline_name"),
                )
            {
                durations.entry(pipeline).or_default().push(end - start);
            }
        }
        let slowest_pipeline = durations
            .into_iter()
            .filter_map(|(name, mut values)| {
                median(&mut values).map(|median_seconds| DigestPipeline {
                    name,
                    median_seconds,
                })
            })
            .max_by_key(|pipeline| pipeline.median_seconds);

        let releases = sqlx::query(
            "SELECT version_name, store, track, state FROM releases \
             WHERE project_id = ?1 AND updated_at >= ?2 AND version_name IS NOT NULL \
             ORDER BY updated_at DESC LIMIT 10",
        )
        .bind(project_id)
        .bind(since)
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| DigestRelease {
            version: row.get("version_name"),
            store: row.get("store"),
            track: row.get("track"),
            state: row.get("state"),
        })
        .collect::<Vec<_>>();

        if !rows.is_empty() || !releases.is_empty() {
            apps.push(DigestApp {
                project_id: project_id.clone(),
                name: name.clone(),
                builds: rows.len() as i64,
                succeeded,
                failed,
                slowest_pipeline,
                releases,
            });
        }

        let expiries = sqlx::query(
            "SELECT 'Certificate for ' || pl.name AS name, s.p12_expires_at AS expires_at \
             FROM pipeline_ios_signing_settings s JOIN pipelines pl ON pl.id = s.pipeline_id \
             WHERE pl.project_id = ?1 AND s.p12_expires_at BETWEEN ?2 AND ?3 \
             UNION ALL \
             SELECT 'Profile ' || COALESCE(pp.profile_name, pp.bundle_id) AS name, pp.expires_at \
             FROM pipeline_ios_provisioning_profiles pp JOIN pipelines pl ON pl.id = pp.pipeline_id \
             WHERE pl.project_id = ?1 AND pp.expires_at BETWEEN ?2 AND ?3",
        )
        .bind(project_id)
        .bind(now)
        .bind(now + EXPIRY_WINDOW)
        .fetch_all(pool)
        .await?;
        for row in expiries {
            expiring.push(DigestExpiry {
                name: row.get("name"),
                project_name: name.clone(),
                expires_at: row.get("expires_at"),
            });
        }
    }
    apps.sort_by_key(|app| std::cmp::Reverse(app.builds));
    expiring.sort_by_key(|expiry| expiry.expires_at);
    Ok(WeeklyDigest {
        period_start: since,
        period_end: now,
        apps,
        expiring,
    })
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn minutes(seconds: i64) -> String {
    let minutes = (seconds + 30) / 60;
    if minutes < 1 {
        format!("{seconds} s")
    } else {
        format!("{minutes} min")
    }
}

fn days_until(at: i64, now: i64) -> String {
    match (at - now) / DAY {
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        days => format!("in {days} days"),
    }
}

/// Subject and HTML body of the digest email.
pub fn digest_email(digest: &WeeklyDigest) -> (String, String) {
    let builds: i64 = digest.apps.iter().map(|app| app.builds).sum();
    let succeeded: i64 = digest.apps.iter().map(|app| app.succeeded).sum();
    let shipped = digest
        .apps
        .iter()
        .flat_map(|app| app.releases.iter())
        .filter(|release| release.state == "complete" || release.state == "rolling")
        .count();
    let subject = format!(
        "Your week in Oore: {builds} build{}, {shipped} release{} out",
        if builds == 1 { "" } else { "s" },
        if shipped == 1 { "" } else { "s" },
    );
    let mut body = String::from(
        "<div style=\"font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;\
         max-width:560px;margin:0 auto;color:#1a1a1a;line-height:1.5\">",
    );
    body.push_str("<h2 style=\"font-size:18px;margin:0 0 4px\">Your week in Oore</h2>");
    if builds > 0 {
        body.push_str(&format!(
            "<p style=\"margin:0 0 16px;color:#555\">{builds} builds finished, {}% succeeded.</p>",
            succeeded * 100 / builds.max(1)
        ));
    } else {
        body.push_str(
            "<p style=\"margin:0 0 16px;color:#555\">A quiet week: no builds finished.</p>",
        );
    }
    for app in &digest.apps {
        body.push_str(&format!(
            "<h3 style=\"font-size:15px;margin:16px 0 4px\">{}</h3><ul style=\"margin:0;padding-left:18px\">",
            html_escape(&app.name)
        ));
        if app.builds > 0 {
            body.push_str(&format!(
                "<li>{} builds, {} succeeded, {} failed</li>",
                app.builds, app.succeeded, app.failed
            ));
        }
        if let Some(pipeline) = &app.slowest_pipeline {
            body.push_str(&format!(
                "<li>Slowest pipeline: {} (about {})</li>",
                html_escape(&pipeline.name),
                minutes(pipeline.median_seconds)
            ));
        }
        for release in &app.releases {
            body.push_str(&format!(
                "<li>{} on {} {}: {}</li>",
                html_escape(&release.version),
                store_words(&release.store),
                html_escape(&release.track),
                release_state_words(&release.state)
            ));
        }
        body.push_str("</ul>");
    }
    if !digest.expiring.is_empty() {
        body.push_str("<h3 style=\"font-size:15px;margin:16px 0 4px\">Expiring soon</h3><ul style=\"margin:0;padding-left:18px\">");
        for expiry in &digest.expiring {
            body.push_str(&format!(
                "<li>{} ({}) expires {}</li>",
                html_escape(&expiry.name),
                html_escape(&expiry.project_name),
                days_until(expiry.expires_at, digest.period_end)
            ));
        }
        body.push_str("</ul>");
    }
    body.push_str(
        "<p style=\"margin:24px 0 0;font-size:12px;color:#888\">You get this because the weekly \
         digest is on in your Oore notification settings.</p></div>",
    );
    (subject, body)
}

/// `GET /v1/users/me/digest` — what this week's digest says.
pub async fn get_digest(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> ApiResult<WeeklyDigest> {
    let digest = digest_for(&state.db, &auth.0.user_id, &auth.0.role, now_unix())
        .await
        .map_err(store_error("Failed to build the digest"))?;
    Ok(Json(digest))
}

/// `POST /v1/users/me/digest/send` — email this week's digest now.
pub async fn send_digest_now(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !email_available(&state.db).await {
        return Err(api_err(
            StatusCode::CONFLICT,
            "email_unavailable",
            "Add an email channel under Settings > Notifications first; personal email uses its server",
        ));
    }
    let digest = digest_for(&state.db, &auth.0.user_id, &auth.0.role, now_unix())
        .await
        .map_err(store_error("Failed to build the digest"))?;
    let (subject, html) = digest_email(&digest);
    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(store_error("Failed to queue the digest"))?;
    queue_email(&mut conn, &auth.0.user_id, &subject, &html)
        .await
        .map_err(store_error("Failed to queue the digest"))?;
    Ok(StatusCode::ACCEPTED)
}

// ── Personal email ──────────────────────────────────────────────

pub async fn queue_email(
    conn: &mut SqliteConnection,
    user_id: &str,
    subject: &str,
    html: &str,
) -> Result<(), sqlx::Error> {
    let now = now_unix();
    sqlx::query(
        "INSERT INTO personal_emails (id, user_id, subject, html_body, next_attempt_at, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(subject)
    .bind(html)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

/// Who a personal notification goes to.
pub enum Audience<'a> {
    /// The person who started a build, by email, with its final status.
    BuildOwner { email: &'a str, status: &'a str },
    /// Everyone who can approve builds in the app.
    Approvers { project_id: &'a str },
    /// Everyone who pinned the app.
    Followers { project_id: &'a str },
}

fn wants(preferences: &NotificationPreferences, audience: &Audience<'_>) -> bool {
    if !preferences.email {
        return false;
    }
    match audience {
        Audience::BuildOwner { status, .. } => match preferences.my_builds {
            PersonalBuildEvents::Off => false,
            PersonalBuildEvents::Failed => matches!(*status, "failed" | "timed_out"),
            PersonalBuildEvents::All => true,
        },
        Audience::Approvers { .. } => preferences.approvals,
        Audience::Followers { .. } => preferences.followed_releases,
    }
}

/// Queue personal email for everyone in `audience` who asked for it.
pub async fn queue_for_audience(
    conn: &mut SqliteConnection,
    audience: Audience<'_>,
    subject: &str,
    html: &str,
) -> Result<(), sqlx::Error> {
    let rows = sqlx::query(
        "SELECT u.id, u.email, u.role, p.preferences_json \
         FROM user_notification_preferences p JOIN users u ON u.id = p.user_id \
         WHERE u.status = 'active'",
    )
    .fetch_all(&mut *conn)
    .await?;
    for row in rows {
        let preferences: NotificationPreferences =
            serde_json::from_str(&row.get::<String, _>("preferences_json")).unwrap_or_default();
        if !wants(&preferences, &audience) {
            continue;
        }
        let user_id: String = row.get("id");
        let email: String = row.get("email");
        let role: String = row.get("role");
        let included = match &audience {
            Audience::BuildOwner { email: owner, .. } => email.eq_ignore_ascii_case(owner),
            Audience::Approvers { project_id } => {
                is_admin(&role)
                    || (role != "qa_viewer"
                        && sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(*) FROM project_members \
                             WHERE user_id = ?1 AND project_id = ?2 AND role = 'maintainer'",
                        )
                        .bind(&user_id)
                        .bind(*project_id)
                        .fetch_one(&mut *conn)
                        .await?
                            > 0)
            }
            Audience::Followers { project_id } => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM user_favorite_projects \
                     WHERE user_id = ?1 AND project_id = ?2",
                )
                .bind(&user_id)
                .bind(*project_id)
                .fetch_one(&mut *conn)
                .await?
                    > 0
            }
        };
        if included {
            queue_email(conn, &user_id, subject, html).await?;
        }
    }
    Ok(())
}

async fn send_due_emails(pool: &SqlitePool, encryption_key: &[u8]) -> anyhow::Result<()> {
    let now = now_unix();
    let rows = sqlx::query(
        "SELECT e.id, e.subject, e.html_body, e.attempts, u.email \
         FROM personal_emails e JOIN users u ON u.id = e.user_id \
         WHERE e.next_attempt_at <= ?1 ORDER BY e.created_at LIMIT 50",
    )
    .bind(now)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(());
    }
    let Some(mut mailer) = instance_mailer(pool, encryption_key).await? else {
        // Nothing can send them; keep them a day in case a channel appears.
        sqlx::query("DELETE FROM personal_emails WHERE created_at < ?1")
            .bind(now - DAY)
            .execute(pool)
            .await?;
        return Ok(());
    };
    for row in rows {
        let id: String = row.get("id");
        mailer.recipients = vec![row.get("email")];
        let sent = crate::notification_dispatch::send_email(
            &mailer,
            &row.get::<String, _>("subject"),
            &row.get::<String, _>("html_body"),
        )
        .await;
        match sent {
            Ok(()) => {
                sqlx::query("DELETE FROM personal_emails WHERE id = ?1")
                    .bind(&id)
                    .execute(pool)
                    .await?;
            }
            Err(error) => {
                let attempts: i64 = row.get::<i64, _>("attempts") + 1;
                warn!(%error, attempts, "personal email failed");
                if attempts >= EMAIL_MAX_ATTEMPTS {
                    sqlx::query("DELETE FROM personal_emails WHERE id = ?1")
                        .bind(&id)
                        .execute(pool)
                        .await?;
                } else {
                    sqlx::query(
                        "UPDATE personal_emails SET attempts = ?2, next_attempt_at = ?3 WHERE id = ?1",
                    )
                    .bind(&id)
                    .bind(attempts)
                    .bind(now + 60 * 2_i64.pow(attempts as u32))
                    .execute(pool)
                    .await?;
                }
            }
        }
    }
    Ok(())
}

/// Monday at or after `DIGEST_HOUR_UTC`, as a Unix time, for `now`'s week.
fn digest_due_since(now: i64) -> i64 {
    // 1970-01-01 was a Thursday; Monday is 4 days later.
    let days = now.div_euclid(DAY);
    let weekday_from_monday = (days - 4).rem_euclid(7);
    (days - weekday_from_monday) * DAY + DIGEST_HOUR_UTC * 3_600
}

async fn queue_due_digests(pool: &SqlitePool) -> anyhow::Result<()> {
    let now = now_unix();
    let due_since = digest_due_since(now);
    if now < due_since {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT u.id, u.role, p.preferences_json, p.last_digest_at \
         FROM user_notification_preferences p JOIN users u ON u.id = p.user_id \
         WHERE u.status = 'active' AND (p.last_digest_at IS NULL OR p.last_digest_at < ?1)",
    )
    .bind(due_since)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let preferences: NotificationPreferences =
            serde_json::from_str(&row.get::<String, _>("preferences_json")).unwrap_or_default();
        let user_id: String = row.get("id");
        if preferences.weekly_digest {
            let digest = digest_for(pool, &user_id, &row.get::<String, _>("role"), now).await?;
            let (subject, html) = digest_email(&digest);
            let mut conn = pool.acquire().await?;
            queue_email(&mut conn, &user_id, &subject, &html).await?;
        }
        sqlx::query(
            "UPDATE user_notification_preferences SET last_digest_at = ?2 WHERE user_id = ?1",
        )
        .bind(&user_id)
        .bind(now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Sends personal email and queues the weekly digest.
pub fn start_personal_mailer(pool: SqlitePool, encryption_key: Vec<u8>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(MAILER_INTERVAL).await;
            if let Err(error) = queue_due_digests(&pool).await {
                error!(%error, "weekly digest scan failed");
            }
            if let Err(error) = send_due_emails(&pool, &encryption_key).await {
                error!(%error, "personal email scan failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_is_due_from_monday_morning() {
        // 2026-11-09 is a Monday.
        let monday = 1_794_182_400;
        let due = digest_due_since(monday + 12 * 3_600);
        assert_eq!(due, monday + DIGEST_HOUR_UTC * 3_600);
        // Sunday night belongs to the week that started six days earlier.
        let sunday = monday - 3_600;
        assert_eq!(
            digest_due_since(sunday),
            monday - 7 * DAY + DIGEST_HOUR_UTC * 3_600
        );
    }

    #[test]
    fn only_failures_reach_people_who_asked_for_failures() {
        let preferences = NotificationPreferences {
            email: true,
            ..NotificationPreferences::default()
        };
        let owner = |status| Audience::BuildOwner {
            email: "a@b.c",
            status,
        };
        assert!(wants(&preferences, &owner("failed")));
        assert!(!wants(&preferences, &owner("succeeded")));
        let no_email = NotificationPreferences::default();
        assert!(!wants(&no_email, &owner("failed")));
    }

    #[test]
    fn the_digest_email_counts_the_week() {
        let digest = WeeklyDigest {
            period_start: 0,
            period_end: 7 * DAY,
            apps: vec![DigestApp {
                project_id: "p".into(),
                name: "Coin <beta>".into(),
                builds: 4,
                succeeded: 3,
                failed: 1,
                slowest_pipeline: Some(DigestPipeline {
                    name: "iOS".into(),
                    median_seconds: 540,
                }),
                releases: vec![DigestRelease {
                    version: "2.1.0".into(),
                    store: "google_play".into(),
                    track: "production".into(),
                    state: "rolling".into(),
                }],
            }],
            expiring: vec![],
        };
        let (subject, html) = digest_email(&digest);
        assert_eq!(subject, "Your week in Oore: 4 builds, 1 release out");
        assert!(html.contains("75% succeeded"));
        assert!(html.contains("Coin &lt;beta&gt;"));
        assert!(html.contains("about 9 min"));
    }

    async fn seeded() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = now_unix();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u1', 'lead@example.com', 'sub', 'owner', 0, 0)".to_string(),
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('u2', 'dev@example.com', 'sub2', 'developer', 0, 0)".to_string(),
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'Shop', 'u1', 0, 0)".to_string(),
            "INSERT INTO project_members (id, project_id, user_id, role, created_by, created_at, updated_at) \
             VALUES ('m1', 'p1', 'u2', 'developer', 'u1', 0, 0)".to_string(),
            "INSERT INTO pipelines (id, project_id, name, execution_config, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'Android', '{\"platforms\":[\"android\",\"ios\"]}', 0, 0)".to_string(),
            format!("INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             trigger_actor, queued_at, started_at, finished_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 1, 'failed', 'manual', 'dev@example.com', {q}, {q}, {f}, {q}, {f})",
             q = now - 900, f = now - 300),
            format!("INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             queued_at, started_at, finished_at, created_at, updated_at) \
             VALUES ('b2', 'p1', 'pl1', 2, 'succeeded', 'manual', {q}, {q}, {f}, {q}, {f})",
             q = now - 800, f = now - 200),
            format!("INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             approval_state, queued_at, created_at, updated_at) \
             VALUES ('b3', 'p1', 'pl1', 3, 'queued', 'manual', 'pending', {n}, {n}, {n})", n = now - 60),
            format!("INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             queued_at, created_at, updated_at) \
             VALUES ('b4', 'p1', 'pl1', 4, 'queued', 'manual', {n}, {n}, {n})", n = now - 30),
            format!("INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
             queued_at, created_at, updated_at) \
             VALUES ('b5', 'p1', 'pl1', 5, 'queued', 'manual', {n}, {n}, {n})", n = now - 20),
            format!("INSERT INTO releases (id, project_id, build_id, store, track, version_name, state, \
             created_at, updated_at) VALUES ('r1', 'p1', 'b2', 'google_play', 'production', '2.1.0', \
             'rolling', {n}, {n})", n = now - 100),
            format!("INSERT INTO release_history (id, release_id, to_state, at) \
             VALUES ('h1', 'r1', 'rolling', {n})", n = now - 100),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(statement.as_str()))
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn the_inbox_digest_and_list_fields_come_from_the_database() {
        let pool = seeded().await;
        let now = now_unix();

        // The developer sees their failure but cannot approve.
        let inbox = inbox_for(&pool, "u2", "dev@example.com", "developer", now)
            .await
            .unwrap();
        assert_eq!(inbox.len(), 1, "{inbox:?}");
        assert_eq!(inbox[0].kind, InboxItemKind::FailedBuild);

        // The owner can approve; pinning the app adds its release change.
        sqlx::query("INSERT INTO user_favorite_projects VALUES ('u1', 'p1', 0)")
            .execute(&pool)
            .await
            .unwrap();
        let inbox = inbox_for(&pool, "u1", "lead@example.com", "owner", now)
            .await
            .unwrap();
        let kinds: Vec<_> = inbox.iter().map(|item| item.kind).collect();
        assert_eq!(kinds, vec![InboxItemKind::Approval, InboxItemKind::Release]);
        assert_eq!(inbox[1].title, "Shop 2.1.0 is rolling out");

        let digest = digest_for(&pool, "u1", "owner", now).await.unwrap();
        assert_eq!(digest.apps.len(), 1);
        assert_eq!((digest.apps[0].builds, digest.apps[0].failed), (2, 1));
        assert_eq!(
            digest.apps[0]
                .slowest_pipeline
                .as_ref()
                .unwrap()
                .median_seconds,
            600
        );

        let row = sqlx::query("SELECT * FROM projects WHERE id = 'p1'")
            .fetch_one(&pool)
            .await
            .unwrap();
        let mut projects = vec![Project {
            id: row.get("id"),
            name: row.get("name"),
            description: None,
            repository_id: None,
            repository_full_name: None,
            repository_avatar_url: None,
            repository_provider: None,
            settings: serde_json::json!({}),
            default_branch: None,
            created_by: "u1".into(),
            created_at: 0,
            updated_at: 0,
            current_user_role: oore_contract::ProjectRole::Maintainer,
            auto_add_pipelines: false,
            is_favorite: false,
            last_build_at: None,
            platforms: vec![],
            live_version: None,
        }];
        decorate_projects(&pool, "u1", &mut projects).await.unwrap();
        assert!(projects[0].is_favorite);
        assert_eq!(projects[0].live_version.as_deref(), Some("2.1.0"));
        assert_eq!(
            projects[0].platforms,
            vec![BuildPlatform::Android, BuildPlatform::Ios]
        );

        // b4 waits behind nothing runnable (b3 is held for approval); b5 is behind b4.
        let first = crate::queue_estimate::estimate(&pool, "b4", now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.ahead, 0);
        let second = crate::queue_estimate::estimate(&pool, "b5", now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.ahead, 1);
        assert_eq!(second.estimated_wait_seconds, Some(600));
        assert!(
            crate::queue_estimate::estimate(&pool, "b3", now)
                .await
                .unwrap()
                .is_none()
        );

        // Email reaches only the owner of a failed build who asked for it.
        sqlx::query(
            "INSERT INTO user_notification_preferences (user_id, preferences_json, updated_at) \
             VALUES ('u2', '{\"my_builds\":\"failed\",\"approvals\":true,\"followed_releases\":true,\
             \"email\":true,\"browser\":true,\"weekly_digest\":true}', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        queue_for_audience(
            &mut conn,
            Audience::BuildOwner {
                email: "dev@example.com",
                status: "failed",
            },
            "s",
            "h",
        )
        .await
        .unwrap();
        queue_for_audience(
            &mut conn,
            Audience::Approvers { project_id: "p1" },
            "s",
            "h",
        )
        .await
        .unwrap();
        drop(conn);
        let queued: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM personal_emails")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(queued, 1);
    }
}
