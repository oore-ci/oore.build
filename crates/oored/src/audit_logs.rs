use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use oore_contract::{AuditLogEntry, ApiError, ListAuditLogsResponse};
use serde::Deserialize;
use sqlx::Row;
use tracing::error;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac::check_permission;
use crate::util::api_err;

type ApiResult<T> = Result<Json<T>, (StatusCode, Json<ApiError>)>;

#[derive(Debug, Deserialize)]
pub struct ListAuditLogsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub actor_id: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub from_ts: Option<i64>,
    pub to_ts: Option<i64>,
}

/// `GET /v1/audit-logs` — paginated, filterable audit log.
pub async fn list_audit_logs(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListAuditLogsQuery>,
) -> ApiResult<ListAuditLogsResponse> {
    check_permission(&state.enforcer, &auth.0.role, "audit_logs", "read").await?;

    let store = state.store.lock().await;
    let pool = store.pool();

    let limit = params.limit.unwrap_or(50).min(200);
    let offset = params.offset.unwrap_or(0);

    // Build dynamic query with filters
    let mut conditions = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();

    if let Some(ref actor_id) = params.actor_id {
        bind_values.push(actor_id.clone());
        conditions.push(format!("a.actor_id = ?{}", bind_values.len()));
    }
    if let Some(ref action) = params.action {
        bind_values.push(action.clone());
        conditions.push(format!("a.action = ?{}", bind_values.len()));
    }
    if let Some(ref resource_type) = params.resource_type {
        bind_values.push(resource_type.clone());
        conditions.push(format!("a.resource_type = ?{}", bind_values.len()));
    }
    if let Some(from_ts) = params.from_ts {
        bind_values.push(from_ts.to_string());
        conditions.push(format!("a.created_at >= ?{}", bind_values.len()));
    }
    if let Some(to_ts) = params.to_ts {
        bind_values.push(to_ts.to_string());
        conditions.push(format!("a.created_at <= ?{}", bind_values.len()));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let count_query = format!("SELECT COUNT(*) FROM audit_logs a {where_clause}");
    let list_query = format!(
        "SELECT a.*, u.email AS actor_email FROM audit_logs a \
         LEFT JOIN users u ON a.actor_id = u.id \
         {where_clause} ORDER BY a.created_at DESC LIMIT ?{} OFFSET ?{}",
        bind_values.len() + 1,
        bind_values.len() + 2
    );

    // Execute count query
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_query);
    for val in &bind_values {
        count_q = count_q.bind(val);
    }
    let total = count_q.fetch_one(pool).await.unwrap_or(0);

    // Execute list query
    let mut list_q = sqlx::query(&list_query);
    for val in &bind_values {
        list_q = list_q.bind(val);
    }
    list_q = list_q.bind(limit).bind(offset);

    let rows = list_q.fetch_all(pool).await.map_err(|e| {
        error!(error = %e, "failed to list audit logs");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "store_error",
            "Failed to list audit logs",
        )
    })?;

    let entries = rows.iter().map(row_to_audit_log_entry).collect();

    Ok(Json(ListAuditLogsResponse { entries, total }))
}

fn row_to_audit_log_entry(row: &sqlx::sqlite::SqliteRow) -> AuditLogEntry {
    AuditLogEntry {
        id: row.get("id"),
        actor_id: row.get("actor_id"),
        actor_email: row.get("actor_email"),
        action: row.get("action"),
        resource_type: row.get("resource_type"),
        resource_id: row.get("resource_id"),
        details: row.get("details"),
        created_at: row.get("created_at"),
    }
}
