//! The audit log: who did what, read back with the names of the records
//! each entry refers to so a reader never has to decode an id.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiError, AuditLogActivityDay, AuditLogActivityResponse, AuditLogCategory, AuditLogEntry,
    ListAuditLogsResponse,
};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use tracing::error;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::util::{api_err, now_unix};

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

const SECONDS_PER_DAY: i64 = 86_400;
const DEFAULT_ACTIVITY_DAYS: i64 = 14;
const MAX_ACTIVITY_DAYS: i64 = 90;

/// Resource types whose entries are about who can get in and what they may do.
const ACCESS_RESOURCE_TYPES: &[&str] = &[
    "auth",
    "user",
    "api_token",
    "local_recovery_capability",
    "local_operator_request",
];
/// App membership is recorded against the app, so it is matched by action.
const ACCESS_ACTION_PREFIX: &str = "project_member_";
/// Housekeeping Oore runs on its own.
const SYSTEM_ACTIONS: &[&str] = &[
    "retention_cleanup_completed",
    "artifact_expiry_cleanup_completed",
];
/// Resource types whose entries are about a build or what was shipped from it.
const BUILD_RESOURCE_TYPES: &[&str] = &[
    "build",
    "release",
    "artifact",
    "artifact_download_token",
    "environment_promotion",
];
/// Shipping actions recorded against an app or a pipeline.
const BUILD_ACTIONS: &[&str] = &[
    "release_freeze_set",
    "release_freeze_lifted",
    "release_notes_updated",
    "latest_install_link_created",
    "latest_install_link_deleted",
    "ios_device_self_registered",
    "pipeline_ios_device_registered",
];
/// Substrings of the actions that record something blocked or failed.
const PROBLEM_ACTION_MARKERS: &[&str] = &["blocked", "failed", "not_allowed"];

/// The kind of activity an entry records. [`category_sql`] is the same rule
/// in SQL, so filtering and labelling cannot disagree.
pub fn audit_category(action: &str, resource_type: &str) -> AuditLogCategory {
    if ACCESS_RESOURCE_TYPES.contains(&resource_type) || action.starts_with(ACCESS_ACTION_PREFIX) {
        AuditLogCategory::Access
    } else if SYSTEM_ACTIONS.contains(&action) {
        AuditLogCategory::System
    } else if BUILD_RESOURCE_TYPES.contains(&resource_type) || BUILD_ACTIONS.contains(&action) {
        AuditLogCategory::Builds
    } else {
        AuditLogCategory::Settings
    }
}

fn sql_list(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| format!("'{value}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn category_sql() -> String {
    format!(
        "CASE \
         WHEN a.resource_type IN ({access_types}) \
           OR substr(a.action, 1, {prefix_len}) = '{prefix}' THEN 'access' \
         WHEN a.action IN ({system_actions}) THEN 'system' \
         WHEN a.resource_type IN ({build_types}) OR a.action IN ({build_actions}) THEN 'builds' \
         ELSE 'settings' END",
        access_types = sql_list(ACCESS_RESOURCE_TYPES),
        prefix_len = ACCESS_ACTION_PREFIX.len(),
        prefix = ACCESS_ACTION_PREFIX,
        system_actions = sql_list(SYSTEM_ACTIONS),
        build_types = sql_list(BUILD_RESOURCE_TYPES),
        build_actions = sql_list(BUILD_ACTIONS),
    )
}

fn problem_sql() -> String {
    PROBLEM_ACTION_MARKERS
        .iter()
        .map(|marker| format!("instr(action, '{marker}') > 0"))
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[derive(Debug, Default, Deserialize)]
pub struct ListAuditLogsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub q: Option<String>,
    pub category: Option<AuditLogCategory>,
    pub resource_type: Option<String>,
    pub from_ts: Option<i64>,
    pub to_ts: Option<i64>,
    pub sort: Option<String>,
    pub direction: Option<String>,
}

fn audit_log_order_clause(
    sort: Option<&str>,
    direction: Option<&str>,
) -> Result<String, (StatusCode, Json<ApiError>)> {
    let column = match sort.unwrap_or("created_at") {
        "created_at" => "a.created_at",
        "actor_email" => "u.email COLLATE NOCASE",
        "action" => "a.action COLLATE NOCASE",
        "resource_type" => "a.resource_type COLLATE NOCASE",
        _ => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "sort must be created_at, actor_email, action, or resource_type",
            ));
        }
    };
    let direction = match direction.unwrap_or("desc") {
        "asc" => "ASC",
        "desc" => "DESC",
        _ => {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_input",
                "direction must be asc or desc",
            ));
        }
    };

    Ok(format!("{column} {direction}, a.id {direction}"))
}

/// A `LIKE` pattern that finds `text` anywhere, with `\` as the escape.
fn contains_pattern(text: &str) -> String {
    let mut pattern = String::with_capacity(text.len() + 2);
    pattern.push('%');
    for character in text.chars() {
        if matches!(character, '%' | '_' | '\\') {
            pattern.push('\\');
        }
        pattern.push(character);
    }
    pattern.push('%');
    pattern
}

/// The `WHERE` clause for a query and the values it binds, in order.
fn audit_log_filters(params: &ListAuditLogsQuery) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut binds: Vec<String> = Vec::new();
    let mut bind = |value: String| {
        binds.push(value);
        binds.len()
    };

    match params.actor_id.as_deref() {
        Some("system") => conditions.push("a.actor_id IS NULL".to_string()),
        Some(actor_id) => conditions.push(format!("a.actor_id = ?{}", bind(actor_id.to_string()))),
        None => {}
    }
    if let Some(action) = &params.action {
        conditions.push(format!("a.action = ?{}", bind(action.clone())));
    }
    if let Some(resource_type) = &params.resource_type {
        conditions.push(format!(
            "a.resource_type = ?{}",
            bind(resource_type.clone())
        ));
    }
    if let Some(category) = params.category {
        conditions.push(format!(
            "{} = ?{}",
            category_sql(),
            bind(category.as_str().to_string())
        ));
    }
    if let Some(from_ts) = params.from_ts {
        conditions.push(format!(
            "a.created_at >= CAST(?{} AS INTEGER)",
            bind(from_ts.to_string())
        ));
    }
    if let Some(to_ts) = params.to_ts {
        conditions.push(format!(
            "a.created_at <= CAST(?{} AS INTEGER)",
            bind(to_ts.to_string())
        ));
    }
    if let Some(query) = params.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        // Actions are stored as `snake_case`, so "role changed" finds `role_changed`.
        let action = bind(contains_pattern(&query.to_lowercase().replace(' ', "_")));
        let text = bind(contains_pattern(query));
        conditions.push(format!(
            "(a.action LIKE ?{action} ESCAPE '\\' \
              OR u.email LIKE ?{text} ESCAPE '\\' \
              OR u.display_name LIKE ?{text} ESCAPE '\\' \
              OR a.details LIKE ?{text} ESCAPE '\\' \
              OR a.resource_id LIKE ?{text} ESCAPE '\\' \
              OR a.resource_id IN (SELECT id FROM projects WHERE name LIKE ?{text} ESCAPE '\\') \
              OR a.resource_id IN (\
                SELECT pl.id FROM pipelines pl JOIN projects pr ON pr.id = pl.project_id \
                WHERE pl.name LIKE ?{text} ESCAPE '\\' OR pr.name LIKE ?{text} ESCAPE '\\') \
              OR a.resource_id IN (\
                SELECT b.id FROM builds b JOIN projects pr ON pr.id = b.project_id \
                WHERE pr.name LIKE ?{text} ESCAPE '\\'))"
        ));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };
    (where_clause, binds)
}

/// One page of audit entries and the number of entries the filters match.
async fn query_audit_logs(
    pool: &SqlitePool,
    params: &ListAuditLogsQuery,
    order_by: &str,
    limit: i64,
    offset: i64,
) -> Result<ListAuditLogsResponse, sqlx::Error> {
    let (where_clause, binds) = audit_log_filters(params);
    let from = "FROM audit_logs a LEFT JOIN users u ON a.actor_id = u.id";

    let count_query = format!("SELECT COUNT(*) {from} {where_clause}");
    let mut count = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_query));
    for value in &binds {
        count = count.bind(value);
    }
    let total = count.fetch_one(pool).await?;

    let list_query = format!(
        "SELECT a.*, u.email AS actor_email, NULLIF(u.display_name, '') AS actor_name \
         {from} {where_clause} ORDER BY {order_by} LIMIT ?{} OFFSET ?{}",
        binds.len() + 1,
        binds.len() + 2
    );
    let mut list = sqlx::query(sqlx::AssertSqlSafe(list_query));
    for value in &binds {
        list = list.bind(value);
    }
    let rows = list.bind(limit).bind(offset).fetch_all(pool).await?;

    let mut entries: Vec<AuditLogEntry> = rows.iter().map(row_to_audit_log_entry).collect();
    name_referenced_records(pool, &mut entries).await?;
    Ok(ListAuditLogsResponse { entries, total })
}

/// `GET /v1/audit-logs` — paginated, filterable audit log.
pub async fn list_audit_logs(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListAuditLogsQuery>,
) -> ApiResult<ListAuditLogsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "audit_logs", "read").await?;

    let requested_limit = params.limit.unwrap_or(50);
    if requested_limit <= 0 {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "limit must be greater than zero",
        ));
    }
    if params.offset.is_some_and(|offset| offset < 0) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "offset must not be negative",
        ));
    }
    let limit = requested_limit.min(200);
    let offset = params.offset.unwrap_or(0);
    let order_by = audit_log_order_clause(params.sort.as_deref(), params.direction.as_deref())?;

    query_audit_logs(&state.db, &params, &order_by, limit, offset)
        .await
        .map(Json)
        .map_err(|e| {
            error!(error = %e, "failed to list audit logs");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to list audit logs",
            )
        })
}

fn row_to_audit_log_entry(row: &sqlx::sqlite::SqliteRow) -> AuditLogEntry {
    let action: String = row.get("action");
    let resource_type: String = row.get("resource_type");
    AuditLogEntry {
        id: row.get("id"),
        actor_id: row.get("actor_id"),
        actor_email: row.get("actor_email"),
        actor_name: row.get("actor_name"),
        category: audit_category(&action, &resource_type),
        action,
        resource_type,
        resource_id: row.get("resource_id"),
        details: row.get("details"),
        project_id: None,
        project_name: None,
        pipeline_id: None,
        pipeline_name: None,
        build_id: None,
        build_number: None,
        names: BTreeMap::new(),
        created_at: row.get("created_at"),
    }
}

/// The ids an entry refers to: its resource, then every short string in its
/// details. Details are JSON for most actions and free text for a few.
fn referenced_ids(entry: &AuditLogEntry) -> Vec<String> {
    let mut ids: Vec<String> = entry.resource_id.iter().cloned().collect();
    let details = entry
        .details
        .as_deref()
        .and_then(|details| serde_json::from_str::<serde_json::Value>(details).ok());
    if let Some(serde_json::Value::Object(fields)) = details {
        for value in fields.values() {
            if let Some(text) = value.as_str()
                && !text.is_empty()
                && text.len() <= 64
                && !ids.iter().any(|id| id == text)
            {
                ids.push(text.to_string());
            }
        }
    }
    ids
}

/// Rows of `table` whose id is in `ids`. `columns` and `table` are constants.
async fn rows_by_id(
    pool: &SqlitePool,
    columns: &str,
    table: &str,
    ids: &BTreeSet<String>,
) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
    let mut rows = Vec::new();
    let ids: Vec<&String> = ids.iter().collect();
    for chunk in ids.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(", ");
        let sql = format!("SELECT {columns} FROM {table} WHERE id IN ({placeholders})");
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
        for id in chunk {
            query = query.bind(*id);
        }
        rows.extend(query.fetch_all(pool).await?);
    }
    Ok(rows)
}

struct BuildRef {
    number: i64,
    pipeline_id: String,
    project_id: String,
}

/// Fill in the app, pipeline and build each entry is about and the current
/// names of the records it refers to. Records deleted since are left out.
async fn name_referenced_records(
    pool: &SqlitePool,
    entries: &mut [AuditLogEntry],
) -> Result<(), sqlx::Error> {
    let entry_ids: Vec<Vec<String>> = entries.iter().map(referenced_ids).collect();
    let mut ids: BTreeSet<String> = entry_ids.iter().flatten().cloned().collect();
    if ids.is_empty() {
        return Ok(());
    }

    let mut names: HashMap<String, String> = HashMap::new();
    // Follow install links and releases back to the build they came from.
    let mut build_of: HashMap<String, String> = HashMap::new();
    let mut artifact_of: HashMap<String, String> = HashMap::new();

    for row in rows_by_id(pool, "id, artifact_id", "artifact_download_tokens", &ids).await? {
        let artifact_id: String = row.get("artifact_id");
        ids.insert(artifact_id.clone());
        artifact_of.insert(row.get("id"), artifact_id);
    }
    for row in rows_by_id(pool, "id, build_id, name", "artifacts", &ids).await? {
        let id: String = row.get("id");
        let build_id: String = row.get("build_id");
        ids.insert(build_id.clone());
        names.insert(id.clone(), row.get("name"));
        build_of.insert(id, build_id);
    }
    for (token_id, artifact_id) in &artifact_of {
        if let Some(build_id) = build_of.get(artifact_id).cloned() {
            build_of.insert(token_id.clone(), build_id);
        }
    }
    for row in rows_by_id(pool, "id, build_id", "releases", &ids).await? {
        let build_id: String = row.get("build_id");
        ids.insert(build_id.clone());
        build_of.insert(row.get("id"), build_id);
    }

    let mut builds: HashMap<String, BuildRef> = HashMap::new();
    for row in rows_by_id(
        pool,
        "id, build_number, pipeline_id, project_id",
        "builds",
        &ids,
    )
    .await?
    {
        let build = BuildRef {
            number: row.get("build_number"),
            pipeline_id: row.get("pipeline_id"),
            project_id: row.get("project_id"),
        };
        ids.insert(build.pipeline_id.clone());
        ids.insert(build.project_id.clone());
        builds.insert(row.get("id"), build);
    }
    let mut pipelines: HashMap<String, (String, String)> = HashMap::new();
    for row in rows_by_id(pool, "id, name, project_id", "pipelines", &ids).await? {
        let project_id: String = row.get("project_id");
        ids.insert(project_id.clone());
        pipelines.insert(row.get("id"), (row.get("name"), project_id));
    }
    let mut projects: HashMap<String, String> = HashMap::new();
    for row in rows_by_id(pool, "id, name", "projects", &ids).await? {
        projects.insert(row.get("id"), row.get("name"));
    }

    for (columns, table) in [
        (
            "id, COALESCE(NULLIF(display_name, ''), email) AS name",
            "users",
        ),
        ("id, name", "runners"),
        (
            "id, COALESCE(NULLIF(display_name, ''), provider) AS name",
            "integrations",
        ),
        ("id, name", "api_tokens"),
        ("id, name", "notification_channels"),
        ("id, name", "tester_groups"),
        ("id, name", "app_store_connect_keys"),
    ] {
        for row in rows_by_id(pool, columns, table, &ids).await? {
            names.insert(row.get("id"), row.get("name"));
        }
    }
    for (id, (name, _)) in &pipelines {
        names.insert(id.clone(), name.clone());
    }
    for (id, name) in &projects {
        names.insert(id.clone(), name.clone());
    }

    for (entry, ids) in entries.iter_mut().zip(&entry_ids) {
        let build_id = ids.iter().find_map(|id| {
            if builds.contains_key(id) {
                Some(id.clone())
            } else {
                build_of
                    .get(id)
                    .filter(|id| builds.contains_key(*id))
                    .cloned()
            }
        });
        let build = build_id.as_ref().and_then(|id| builds.get(id));
        let pipeline_id = build
            .map(|build| build.pipeline_id.clone())
            .or_else(|| ids.iter().find(|id| pipelines.contains_key(*id)).cloned())
            .filter(|id| pipelines.contains_key(id));
        let project_id = build
            .map(|build| build.project_id.clone())
            .or_else(|| {
                pipeline_id
                    .as_ref()
                    .and_then(|id| pipelines.get(id))
                    .map(|(_, project_id)| project_id.clone())
            })
            .or_else(|| ids.iter().find(|id| projects.contains_key(*id)).cloned())
            .filter(|id| projects.contains_key(id));

        entry.build_number = build.map(|build| build.number);
        entry.build_id = build_id;
        entry.pipeline_name = pipeline_id
            .as_ref()
            .and_then(|id| pipelines.get(id))
            .map(|(name, _)| name.clone());
        entry.pipeline_id = pipeline_id;
        entry.project_name = project_id.as_ref().and_then(|id| projects.get(id)).cloned();
        entry.project_id = project_id;
        entry.names = ids
            .iter()
            .filter_map(|id| names.get(id).map(|name| (id.clone(), name.clone())))
            .collect();
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct AuditLogActivityQuery {
    pub days: Option<i64>,
    pub utc_offset_minutes: Option<i64>,
}

/// Entries per calendar day over the `days` ending at `now`, with days split
/// at midnight in the caller's time zone.
async fn audit_log_activity(
    pool: &SqlitePool,
    now: i64,
    days: i64,
    offset_minutes: i64,
) -> Result<AuditLogActivityResponse, sqlx::Error> {
    let offset = offset_minutes * 60;
    let first_day = (now + offset).div_euclid(SECONDS_PER_DAY) - (days - 1);
    let since = first_day * SECONDS_PER_DAY - offset;

    let rows = sqlx::query(
        "SELECT (created_at + ?1) / 86400 AS day, COUNT(*) AS count \
         FROM audit_logs WHERE created_at >= ?2 GROUP BY day",
    )
    .bind(offset)
    .bind(since)
    .fetch_all(pool)
    .await?;
    let counts: HashMap<i64, i64> = rows
        .iter()
        .map(|row| (row.get("day"), row.get("count")))
        .collect();

    let summary = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) AS total, COUNT(DISTINCT actor_id) AS actors, \
         COALESCE(SUM(CASE WHEN {} THEN 1 ELSE 0 END), 0) AS problems \
         FROM audit_logs WHERE created_at >= ?1",
        problem_sql()
    )))
    .bind(since)
    .fetch_one(pool)
    .await?;

    let days = (first_day..first_day + days)
        .map(|day| AuditLogActivityDay {
            date: chrono::DateTime::from_timestamp(day * SECONDS_PER_DAY, 0)
                .map(|date| date.date_naive().to_string())
                .unwrap_or_default(),
            count: counts.get(&day).copied().unwrap_or(0),
        })
        .collect();
    Ok(AuditLogActivityResponse {
        days,
        total: summary.get("total"),
        actors: summary.get("actors"),
        problems: summary.get("problems"),
    })
}

/// `GET /v1/audit-logs/activity` — entries per day over the recent past.
pub async fn get_audit_log_activity(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<AuditLogActivityQuery>,
) -> ApiResult<AuditLogActivityResponse> {
    check_permission(&state.enforcer, &auth.0.role, "audit_logs", "read").await?;

    let days = params.days.unwrap_or(DEFAULT_ACTIVITY_DAYS);
    if !(1..=MAX_ACTIVITY_DAYS).contains(&days) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "days must be between 1 and 90",
        ));
    }
    let offset_minutes = params.utc_offset_minutes.unwrap_or(0);
    if !(-720..=840).contains(&offset_minutes) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_input",
            "utc_offset_minutes must be between -720 and 840",
        ));
    }

    audit_log_activity(&state.db, now_unix(), days, offset_minutes)
        .await
        .map(Json)
        .map_err(|e| {
            error!(error = %e, "failed to summarize audit activity");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                "Failed to summarize audit activity",
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = SECONDS_PER_DAY;

    async fn pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, display_name, role, created_at, updated_at) \
             VALUES ('u1', 'alex@example.com', 's1', 'Alex Chen', 'owner', 0, 0), \
                    ('u2', 'sam@example.com', 's2', NULL, 'developer', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'Shop', 'u1', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'iOS Release', 0, 0)",
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
                                 queued_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 482, 'succeeded', 'manual', 0, 0, 0)",
            "INSERT INTO artifacts (id, build_id, name, file_path, created_at) \
             VALUES ('a1', 'b1', 'shop.ipa', 'builds/b1/shop.ipa', 0)",
            "INSERT INTO artifact_download_tokens (id, artifact_id, token_hash, prefix, created_by, \
                                                   expires_at, created_at) \
             VALUES ('t1', 'a1', 'hash', 'bbd83621', 'u1', 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        pool
    }

    async fn record(
        pool: &SqlitePool,
        at: i64,
        actor: Option<&str>,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        details: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO audit_logs (actor_id, action, resource_type, resource_id, details, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(actor)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(details)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn list(pool: &SqlitePool, params: ListAuditLogsQuery) -> ListAuditLogsResponse {
        query_audit_logs(pool, &params, "a.created_at DESC, a.id DESC", 50, 0)
            .await
            .unwrap()
    }

    fn actions(response: &ListAuditLogsResponse) -> Vec<&str> {
        response
            .entries
            .iter()
            .map(|entry| entry.action.as_str())
            .collect()
    }

    #[tokio::test]
    async fn an_install_link_is_traced_back_to_its_build_pipeline_and_app() {
        let pool = pool().await;
        record(
            &pool,
            10,
            Some("u1"),
            "artifact_install_link_generated",
            "artifact_download_token",
            Some("t1"),
            Some(r#"{"artifact_id":"a1","platform":"ios","token_prefix":"bbd83621"}"#),
        )
        .await;

        let response = list(&pool, ListAuditLogsQuery::default()).await;
        let entry = &response.entries[0];
        assert_eq!(entry.actor_name.as_deref(), Some("Alex Chen"));
        assert_eq!(entry.category, AuditLogCategory::Builds);
        assert_eq!(entry.build_id.as_deref(), Some("b1"));
        assert_eq!(entry.build_number, Some(482));
        assert_eq!(entry.pipeline_name.as_deref(), Some("iOS Release"));
        assert_eq!(entry.project_name.as_deref(), Some("Shop"));
        assert_eq!(entry.names.get("a1").map(String::as_str), Some("shop.ipa"));
    }

    #[tokio::test]
    async fn people_named_in_details_are_resolved_and_deleted_records_are_left_out() {
        let pool = pool().await;
        record(
            &pool,
            10,
            Some("u1"),
            "project_member_added",
            "project",
            Some("p1"),
            Some(r#"{"added_user_id":"u2","role":"developer"}"#),
        )
        .await;
        record(
            &pool,
            20,
            Some("u1"),
            "pipeline_deleted",
            "pipeline",
            Some("gone"),
            None,
        )
        .await;
        record(
            &pool,
            30,
            Some("u1"),
            "api_token_created",
            "api_token",
            Some("k1"),
            Some("name=ci"),
        )
        .await;

        let response = list(&pool, ListAuditLogsQuery::default()).await;
        let [token, deleted, member] = &response.entries[..] else {
            panic!("expected three entries");
        };
        assert_eq!(member.category, AuditLogCategory::Access);
        assert_eq!(member.project_name.as_deref(), Some("Shop"));
        assert_eq!(
            member.names.get("u2").map(String::as_str),
            Some("sam@example.com")
        );
        assert!(deleted.names.is_empty());
        assert_eq!(deleted.pipeline_id, None);
        assert!(token.names.is_empty());
    }

    #[tokio::test]
    async fn search_matches_action_words_people_details_and_app_names() {
        let pool = pool().await;
        record(
            &pool,
            10,
            Some("u1"),
            "role_changed",
            "user",
            Some("u2"),
            None,
        )
        .await;
        record(
            &pool,
            20,
            Some("u2"),
            "build_canceled",
            "build",
            Some("b1"),
            None,
        )
        .await;
        record(
            &pool,
            30,
            None,
            "trusted_proxy_login_blocked_untrusted_peer",
            "auth",
            None,
            Some(r#"{"peer_ip":"203.0.113.44"}"#),
        )
        .await;

        let search = async |q: &str| {
            let response = list(
                &pool,
                ListAuditLogsQuery {
                    q: Some(q.to_string()),
                    ..Default::default()
                },
            )
            .await;
            assert_eq!(response.total, response.entries.len() as i64);
            response
        };
        assert_eq!(actions(&search("Role Changed").await), ["role_changed"]);
        assert_eq!(actions(&search("alex").await), ["role_changed"]);
        assert_eq!(actions(&search("203.0.113").await).len(), 1);
        assert_eq!(actions(&search("shop").await), ["build_canceled"]);
        // `_` and `%` are text, not wildcards.
        assert!(actions(&search("r_le").await).is_empty());
        assert!(actions(&search("%").await).is_empty());
    }

    #[tokio::test]
    async fn the_category_filter_agrees_with_the_category_on_each_entry() {
        let pool = pool().await;
        let recorded = [
            ("trusted_proxy_login_succeeded", "auth"),
            ("project_member_removed", "project"),
            ("api_token_revoked", "api_token"),
            ("artifact_expiry_cleanup_completed", "artifact"),
            ("retention_cleanup_completed", "retention_policy"),
            ("build_created", "build"),
            ("release_freeze_set", "project"),
            ("promotion_requested", "environment_promotion"),
            ("pipeline_updated", "pipeline"),
            ("update_retention_policy", "retention_policy"),
            ("runner_renamed", "runner"),
        ];
        for (at, (action, resource_type)) in recorded.iter().enumerate() {
            record(&pool, at as i64, None, action, resource_type, None, None).await;
        }

        let mut seen = 0;
        for category in [
            AuditLogCategory::Access,
            AuditLogCategory::Builds,
            AuditLogCategory::Settings,
            AuditLogCategory::System,
        ] {
            let response = list(
                &pool,
                ListAuditLogsQuery {
                    category: Some(category),
                    ..Default::default()
                },
            )
            .await;
            assert!(!response.entries.is_empty(), "{category:?} matched nothing");
            for entry in &response.entries {
                assert_eq!(entry.category, category, "{}", entry.action);
            }
            seen += response.entries.len();
        }
        assert_eq!(seen, recorded.len());
    }

    #[tokio::test]
    async fn the_system_actor_filter_returns_entries_nobody_made() {
        let pool = pool().await;
        record(
            &pool,
            10,
            Some("u1"),
            "build_canceled",
            "build",
            Some("b1"),
            None,
        )
        .await;
        record(
            &pool,
            20,
            None,
            "store_upload_completed",
            "build",
            Some("b1"),
            None,
        )
        .await;

        let response = list(
            &pool,
            ListAuditLogsQuery {
                actor_id: Some("system".to_string()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(actions(&response), ["store_upload_completed"]);
    }

    #[tokio::test]
    async fn activity_counts_days_in_the_callers_time_zone() {
        let pool = pool().await;
        // 2026-11-16 00:00 UTC. In UTC+05:30 the local day starts 5.5 h earlier.
        let midnight = 1_794_787_200;
        record(
            &pool,
            midnight - 6 * 3600,
            Some("u1"),
            "build_canceled",
            "build",
            None,
            None,
        )
        .await;
        record(
            &pool,
            midnight - 3600,
            Some("u1"),
            "role_changed",
            "user",
            None,
            None,
        )
        .await;
        record(
            &pool,
            midnight + 3600,
            Some("u2"),
            "build_created",
            "build",
            None,
            None,
        )
        .await;
        record(
            &pool,
            midnight + 7200,
            None,
            "local_recovery_login_failed",
            "auth",
            None,
            None,
        )
        .await;
        record(
            &pool,
            midnight - 5 * DAY,
            Some("u1"),
            "build_created",
            "build",
            None,
            None,
        )
        .await;

        let now = midnight + 12 * 3600;
        let utc = audit_log_activity(&pool, now, 3, 0).await.unwrap();
        let days: Vec<(&str, i64)> = utc
            .days
            .iter()
            .map(|d| (d.date.as_str(), d.count))
            .collect();
        assert_eq!(
            days,
            [("2026-11-14", 0), ("2026-11-15", 2), ("2026-11-16", 2)]
        );
        assert_eq!((utc.total, utc.actors, utc.problems), (4, 2, 1));

        let india = audit_log_activity(&pool, now, 3, 330).await.unwrap();
        let days: Vec<(&str, i64)> = india
            .days
            .iter()
            .map(|d| (d.date.as_str(), d.count))
            .collect();
        assert_eq!(
            days,
            [("2026-11-14", 0), ("2026-11-15", 1), ("2026-11-16", 3)]
        );
    }
}
