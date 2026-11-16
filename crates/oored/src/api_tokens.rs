use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use oore_contract::{
    ApiTokenSummary, CreateApiTokenRequest, CreateApiTokenResponse, ListApiTokensResponse,
    RevokeApiTokenResponse,
};
use serde::Deserialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

use crate::AppState;
use crate::extractors::AuthUser;
use crate::rbac;
use crate::session::{Ability, AuthSource, SessionInfo, TokenGrant};
use crate::store::write_audit_log;
use crate::token::{api_token_display_prefix, generate_api_token, hash_token};
use crate::util::{api_err, now_unix};

// ── Role hierarchy ───────────────────────────────────────────────

fn role_level(role: &str) -> u8 {
    match role {
        "owner" => 4,
        "admin" => 3,
        "developer" => 2,
        "qa_viewer" => 1,
        _ => 0,
    }
}

const VALID_ROLES: &[&str] = &["owner", "admin", "developer", "qa_viewer"];

fn effective_token_role(token_role: &str, current_role: &str) -> Option<String> {
    if !VALID_ROLES.contains(&token_role) || !VALID_ROLES.contains(&current_role) {
        return None;
    }

    Some(
        if role_level(token_role) <= role_level(current_role) {
            token_role
        } else {
            current_role
        }
        .to_string(),
    )
}

// ── DB helpers ───────────────────────────────────────────────────

/// Abilities a new token gets when none are chosen.
pub const DEFAULT_ABILITIES: [Ability; 2] = [Ability::Read, Ability::Build];

/// What a new token is limited to, beyond its role.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenLimits {
    /// `None` stores no list, so the role implies the abilities, as for
    /// tokens made before abilities existed.
    pub abilities: Option<Vec<Ability>>,
    /// Empty means every app the creator reaches.
    pub project_ids: Vec<String>,
}

pub struct CreatedApiToken {
    pub id: String,
    pub token: String,
    pub prefix: String,
    pub created_at: i64,
}

pub async fn create_api_token(
    pool: &SqlitePool,
    created_by: &str,
    name: &str,
    role: &str,
    expires_at: Option<i64>,
    limits: &TokenLimits,
) -> Result<CreatedApiToken, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    let token = generate_api_token();
    let hashed = hash_token(&token);
    let prefix = api_token_display_prefix(&token);
    let now = now_unix();
    let abilities = limits
        .abilities
        .as_ref()
        .map(|abilities| encode_list(abilities.iter().map(|a| a.as_str())));
    let project_ids = (!limits.project_ids.is_empty())
        .then(|| encode_list(limits.project_ids.iter().map(String::as_str)));

    sqlx::query(
        "INSERT INTO api_tokens (id, name, token_hash, prefix, created_by, role, expires_at, created_at, abilities, project_ids) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&id)
    .bind(name)
    .bind(&hashed)
    .bind(&prefix)
    .bind(created_by)
    .bind(role)
    .bind(expires_at)
    .bind(now)
    .bind(abilities)
    .bind(project_ids)
    .execute(pool)
    .await?;

    Ok(CreatedApiToken {
        id,
        token,
        prefix,
        created_at: now,
    })
}

fn encode_list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    serde_json::to_string(&items.collect::<Vec<_>>()).unwrap_or_else(|_| "[]".to_string())
}

/// Abilities stored on a token, or `None` for a token made before abilities
/// existed. An unreadable list grants nothing rather than everything.
fn decode_abilities(raw: Option<&str>) -> Option<Vec<Ability>> {
    let raw = raw?;
    let names: Vec<String> = serde_json::from_str(raw).unwrap_or_default();
    let mut abilities: Vec<Ability> = names.iter().filter_map(|n| n.parse().ok()).collect();
    abilities.sort();
    abilities.dedup();
    Some(abilities)
}

fn decode_project_ids(raw: Option<&str>) -> Vec<String> {
    raw.and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
        .unwrap_or_default()
}

/// Abilities a token holds: the stored list, or what its role implies.
pub(crate) fn token_abilities(stored: Option<&str>, role: &str) -> Vec<Ability> {
    decode_abilities(stored).unwrap_or_else(|| Ability::implied_by_role(role))
}

pub async fn validate_api_token(
    pool: &SqlitePool,
    token: &str,
) -> Result<Option<SessionInfo>, sqlx::Error> {
    let hashed = hash_token(token);
    validate_api_token_hash(pool, &hashed).await
}

pub(crate) async fn validate_api_token_hash(
    pool: &SqlitePool,
    token_hash: &str,
) -> Result<Option<SessionInfo>, sqlx::Error> {
    let now = now_unix();

    let row = sqlx::query(
        "SELECT t.id, t.name AS token_name, t.role AS token_role, t.expires_at AS token_expires_at, \
                t.abilities, t.project_ids, \
                u.id AS user_id, u.email, u.oidc_subject, u.role AS current_role \
         FROM api_tokens t \
         JOIN users u ON u.id = t.created_by \
         WHERE t.token_hash = ?1 \
           AND t.revoked_at IS NULL \
           AND (t.expires_at IS NULL OR t.expires_at > ?2) \
           AND u.status = 'active'",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|r| {
        let token_expires_at: Option<i64> = r.get("token_expires_at");
        let token_role: String = r.get("token_role");
        let current_role: String = r.get("current_role");
        let abilities: Option<String> = r.get("abilities");
        let project_ids: Option<String> = r.get("project_ids");
        let grant = TokenGrant {
            id: r.get("id"),
            name: r.get("token_name"),
            abilities: token_abilities(abilities.as_deref(), &token_role),
            project_ids: decode_project_ids(project_ids.as_deref()),
            approval_abilities: Vec::new(),
        };
        Some(SessionInfo {
            user_id: r.get("user_id"),
            email: r.get("email"),
            oidc_subject: r.get("oidc_subject"),
            role: effective_token_role(&token_role, &current_role)?,
            expires_at: token_expires_at.unwrap_or(i64::MAX),
            auth_source: AuthSource::ApiToken,
            token: Some(Arc::new(grant)),
        })
    }))
}

pub async fn update_last_used(pool: &SqlitePool, token_hash: &str) {
    let now = now_unix();
    if let Err(e) = sqlx::query("UPDATE api_tokens SET last_used_at = ?1 WHERE token_hash = ?2")
        .bind(now)
        .bind(token_hash)
        .execute(pool)
        .await
    {
        error!(error = %e, "failed to update api_token last_used_at");
    }
}

// ── Handlers ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListApiTokensQuery {
    pub q: Option<String>,
    pub sort: Option<String>,
    pub direction: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_api_tokens_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Query(params): Query<ListApiTokensQuery>,
) -> Result<Json<ListApiTokensResponse>, (StatusCode, Json<oore_contract::ApiError>)> {
    rbac::check_permission(&state.enforcer, &auth.0.role, "api_tokens", "read").await?;

    let pool = state.db.clone();
    let now = now_unix();

    let is_admin = auth.0.role == "owner" || auth.0.role == "admin";

    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let search = params.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let status_sql = format!(
        "CASE WHEN t.revoked_at IS NOT NULL THEN 'revoked' WHEN t.expires_at IS NOT NULL AND t.expires_at <= {now} THEN 'expired' ELSE 'active' END"
    );
    let order = match params.sort.as_deref() {
        Some("name") => "t.name",
        Some("role") => "t.role",
        Some("status") => status_sql.as_str(),
        Some("last_used_at") => "COALESCE(t.last_used_at, 0)",
        _ => "t.created_at",
    };
    let direction = if params.direction.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };

    let mut count = QueryBuilder::<Sqlite>::new(
        "SELECT COUNT(*) FROM api_tokens t JOIN users u ON u.id = t.created_by WHERE 1 = 1",
    );
    if !is_admin {
        count
            .push(" AND t.created_by = ")
            .push_bind(&auth.0.user_id);
    }
    if let Some(search) = search {
        count
            .push(" AND lower(t.name || ' ' || t.prefix || ' ' || t.role || ' ' || u.email || ' ' || ")
            .push(&status_sql)
            .push(") LIKE ")
            .push_bind(format!("%{}%", search.to_lowercase()));
    }
    let total: i64 = count
        .build_query_scalar()
        .fetch_one(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to count api tokens");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_error",
                "Failed to list API tokens",
            )
        })?;

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT t.id, t.name, t.prefix, t.role, t.created_by, t.created_at, t.expires_at, t.last_used_at, t.revoked_at, t.abilities, t.project_ids, u.email AS created_by_email FROM api_tokens t JOIN users u ON u.id = t.created_by WHERE 1 = 1",
    );
    if !is_admin {
        query
            .push(" AND t.created_by = ")
            .push_bind(&auth.0.user_id);
    }
    if let Some(search) = search {
        query
            .push(" AND lower(t.name || ' ' || t.prefix || ' ' || t.role || ' ' || u.email || ' ' || ")
            .push(&status_sql)
            .push(") LIKE ")
            .push_bind(format!("%{}%", search.to_lowercase()));
    }
    query
        .push(" ORDER BY ")
        .push(order)
        .push(" ")
        .push(direction)
        .push(", t.id ASC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = query.build().fetch_all(&pool).await;

    let rows = rows.map_err(|e| {
        error!(error = %e, "failed to list api tokens");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_error",
            "Failed to list API tokens",
        )
    })?;

    let tokens: Vec<ApiTokenSummary> = rows
        .iter()
        .map(|r| {
            let expires_at: Option<i64> = r.get("expires_at");
            let revoked_at: Option<i64> = r.get("revoked_at");
            let role: String = r.get("role");
            let abilities: Option<String> = r.get("abilities");
            let project_ids: Option<String> = r.get("project_ids");
            ApiTokenSummary {
                id: r.get("id"),
                name: r.get("name"),
                prefix: r.get("prefix"),
                abilities: token_abilities(abilities.as_deref(), &role),
                abilities_chosen: abilities.is_some(),
                project_ids: decode_project_ids(project_ids.as_deref()),
                role,
                created_by: r.get("created_by"),
                created_by_email: r.get("created_by_email"),
                created_at: r.get("created_at"),
                expires_at,
                last_used_at: r.get("last_used_at"),
                is_expired: expires_at.is_some_and(|ea| ea <= now),
                is_revoked: revoked_at.is_some(),
            }
        })
        .collect();

    Ok(Json(ListApiTokensResponse { tokens, total }))
}

pub async fn create_api_token_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Json(req): Json<CreateApiTokenRequest>,
) -> Result<Json<CreateApiTokenResponse>, (StatusCode, Json<oore_contract::ApiError>)> {
    rbac::check_permission(&state.enforcer, &auth.0.role, "api_tokens", "write").await?;

    // Validate name
    let name = req.name.trim();
    if name.is_empty() {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_name",
            "Token name must not be empty",
        ));
    }

    // Validate role
    if !VALID_ROLES.contains(&req.role.as_str()) {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_role",
            format!(
                "Invalid role '{}'. Must be one of: owner, admin, developer, qa_viewer",
                req.role
            ),
        ));
    }

    // Cannot create a token with a higher role than your own
    if role_level(&req.role) > role_level(&auth.0.role) {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "role_escalation",
            "Cannot create a token with a higher role than your own",
        ));
    }

    if let Some(expires_at) = req.expires_at
        && expires_at <= now_unix()
    {
        return Err(api_err(
            StatusCode::BAD_REQUEST,
            "invalid_expiry",
            "Token expiry must be in the future",
        ));
    }

    let pool = state.db.clone();
    let limits = resolve_token_limits(
        &pool,
        &auth.0,
        req.abilities.as_deref(),
        req.project_ids.as_deref(),
    )
    .await?;

    let created = create_api_token(
        &pool,
        &auth.0.user_id,
        name,
        &req.role,
        req.expires_at,
        &limits,
    )
    .await
    .map_err(|e| {
        error!(error = %e, "failed to create api token");
        api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "db_error",
            "Failed to create API token",
        )
    })?;
    let abilities = limits
        .abilities
        .clone()
        .unwrap_or_else(|| Ability::implied_by_role(&req.role));

    // Audit log
    let mut details = format!(
        "name={}, role={}, prefix={}, abilities={}",
        name,
        req.role,
        created.prefix,
        abilities
            .iter()
            .map(|a| a.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    );
    if !limits.project_ids.is_empty() {
        details.push_str(&format!(", projects={}", limits.project_ids.join(" ")));
    }
    if let Err(e) = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "api_token_created",
        "api_token",
        Some(&created.id),
        Some(&details),
    )
    .await
    {
        error!(error = %e, "failed to write audit log for api token creation");
    }

    info!(
        user_id = %auth.0.user_id,
        token_id = %created.id,
        role = %req.role,
        "API token created"
    );

    Ok(Json(CreateApiTokenResponse {
        id: created.id,
        name: name.to_string(),
        prefix: created.prefix,
        role: req.role,
        created_at: created.created_at,
        expires_at: req.expires_at,
        abilities,
        project_ids: limits.project_ids,
        token: created.token,
    }))
}

/// Check and normalise the abilities and app list asked for a new token.
///
/// A token made with another token can hold no more than that token: its
/// abilities and app list default to the caller's and may only narrow them.
pub(crate) async fn resolve_token_limits(
    pool: &SqlitePool,
    creator: &SessionInfo,
    abilities: Option<&[Ability]>,
    project_ids: Option<&[String]>,
) -> Result<TokenLimits, (StatusCode, Json<oore_contract::ApiError>)> {
    let caller = creator.token.as_deref();

    let mut abilities: Option<Vec<Ability>> = match abilities {
        Some(list) => {
            if list.is_empty() {
                return Err(api_err(
                    StatusCode::BAD_REQUEST,
                    "invalid_abilities",
                    "Choose at least one ability, or leave abilities out for every ability the role allows",
                ));
            }
            // Every other ability needs to read what it acts on.
            let mut list: Vec<Ability> = list.to_vec();
            list.push(Ability::Read);
            list.sort();
            list.dedup();
            Some(list)
        }
        // New tokens default to reading and building. Anything more must be
        // chosen.
        None => Some(
            DEFAULT_ABILITIES
                .iter()
                .copied()
                .filter(|a| caller.is_none_or(|grant| grant.has_ability(*a)))
                .collect(),
        ),
    };
    if let (Some(list), Some(grant)) = (abilities.as_mut(), caller)
        && let Some(extra) = list.iter().find(|a| !grant.has_ability(**a))
    {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "ability_escalation",
            format!(
                "This token cannot make a token with the '{}' ability, because it does not hold it",
                extra.as_str()
            ),
        ));
    }

    let mut ids: Vec<String> = project_ids
        .unwrap_or_default()
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    ids.sort();
    ids.dedup();
    if let Some(grant) = caller
        && !grant.project_ids.is_empty()
    {
        if ids.is_empty() {
            ids = grant.project_ids.clone();
        } else if let Some(outside) = ids.iter().find(|id| !grant.allows_project(id)) {
            return Err(api_err(
                StatusCode::FORBIDDEN,
                "project_escalation",
                format!(
                    "This token cannot reach app {outside}, so it cannot give a token access to it"
                ),
            ));
        }
    }
    for project_id in &ids {
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM projects WHERE id = ?1")
            .bind(project_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                error!(error = %e, "failed to check token project");
                api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "db_error",
                    "Failed to create API token",
                )
            })?;
        let effective = if exists.is_none() {
            crate::project_rbac::EffectiveProjectRole::None
        } else {
            crate::project_rbac::resolve_effective_project_role(
                pool,
                &creator.user_id,
                &creator.role,
                project_id,
                &creator.auth_source,
            )
            .await?
        };
        if effective == crate::project_rbac::EffectiveProjectRole::None {
            return Err(api_err(
                StatusCode::BAD_REQUEST,
                "invalid_project",
                format!("App {project_id} does not exist or you cannot reach it"),
            ));
        }
    }

    Ok(TokenLimits {
        abilities,
        project_ids: ids,
    })
}

pub async fn revoke_api_token_handler(
    State(state): State<Arc<AppState>>,
    auth: AuthUser,
    Path(token_id): Path<String>,
) -> Result<Json<RevokeApiTokenResponse>, (StatusCode, Json<oore_contract::ApiError>)> {
    rbac::check_permission(&state.enforcer, &auth.0.role, "api_tokens", "delete").await?;

    let pool = state.db.clone();

    // Fetch the token to check ownership
    let row = sqlx::query("SELECT created_by, revoked_at FROM api_tokens WHERE id = ?1")
        .bind(&token_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to query api token");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_error",
                "Failed to query API token",
            )
        })?;

    let row =
        row.ok_or_else(|| api_err(StatusCode::NOT_FOUND, "not_found", "API token not found"))?;

    let created_by: String = row.get("created_by");
    let revoked_at: Option<i64> = row.get("revoked_at");

    if revoked_at.is_some() {
        return Ok(Json(RevokeApiTokenResponse { revoked: true }));
    }

    // Non-admin users can only revoke their own tokens
    let is_admin = auth.0.role == "owner" || auth.0.role == "admin";
    if !is_admin && created_by != auth.0.user_id {
        return Err(api_err(
            StatusCode::FORBIDDEN,
            "permission_denied",
            "You can only revoke your own API tokens",
        ));
    }

    let now = now_unix();
    sqlx::query("UPDATE api_tokens SET revoked_at = ?1 WHERE id = ?2")
        .bind(now)
        .bind(&token_id)
        .execute(&pool)
        .await
        .map_err(|e| {
            error!(error = %e, "failed to revoke api token");
            api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "db_error",
                "Failed to revoke API token",
            )
        })?;

    // Audit log
    if let Err(e) = write_audit_log(
        &pool,
        Some(&auth.0.user_id),
        "api_token_revoked",
        "api_token",
        Some(&token_id),
        None,
    )
    .await
    {
        error!(error = %e, "failed to write audit log for api token revocation");
    }

    info!(
        user_id = %auth.0.user_id,
        token_id = %token_id,
        "API token revoked"
    );

    Ok(Json(RevokeApiTokenResponse { revoked: true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_rbac::{EffectiveProjectRole, ProjectPermission, check_project_permission};
    use oore_contract::ProjectRole;
    use serde_json::{Value, json};

    struct Daemon {
        base: String,
        admin: String,
        pool: SqlitePool,
        client: reqwest::Client,
        _dir: tempfile::TempDir,
    }

    impl Daemon {
        async fn get(&self, token: &str, path: &str) -> (u16, Value) {
            let response = self
                .client
                .get(format!("{}{path}", self.base))
                .bearer_auth(token)
                .send()
                .await
                .unwrap();
            let status = response.status().as_u16();
            (status, response.json().await.unwrap_or(Value::Null))
        }

        async fn send(&self, method: &str, token: &str, path: &str, body: Value) -> (u16, Value) {
            let url = format!("{}{path}", self.base);
            let request = match method {
                "POST" => self.client.post(url),
                "PUT" => self.client.put(url),
                "DELETE" => self.client.delete(url),
                _ => unreachable!(),
            };
            let response = request.bearer_auth(token).json(&body).send().await.unwrap();
            let status = response.status().as_u16();
            (status, response.json().await.unwrap_or(Value::Null))
        }

        async fn token(&self, body: Value) -> Value {
            let (status, created) = self.send("POST", &self.admin, "/v1/api-tokens", body).await;
            assert_eq!(status, 200, "{created}");
            created
        }
    }

    async fn daemon() -> Daemon {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::store::SetupStore::connect(dir.path().join("data/oore.db"))
            .await
            .unwrap();
        let pool = store.pool().clone();
        for statement in [
            "INSERT INTO users (id, email, oidc_subject, role, created_at, updated_at) \
             VALUES ('adm', 'admin@example.com', 'adm', 'admin', 0, 0)",
            "INSERT INTO projects (id, name, created_by, created_at, updated_at) \
             VALUES ('p1', 'Kite', 'adm', 0, 0), ('p2', 'Coin', 'adm', 0, 0)",
            "INSERT INTO pipelines (id, project_id, name, created_at, updated_at) \
             VALUES ('pl1', 'p1', 'Android', 0, 0), ('pl2', 'p2', 'Android', 0, 0)",
            "INSERT INTO builds (id, project_id, pipeline_id, build_number, status, trigger_type, \
               queued_at, created_at, updated_at) \
             VALUES ('b1', 'p1', 'pl1', 1, 'running', 'manual', 0, 0, 0), \
                    ('b2', 'p2', 'pl2', 1, 'running', 'manual', 0, 0, 0)",
        ] {
            sqlx::query(statement).execute(&pool).await.unwrap();
        }
        let sessions = crate::session::SessionStore::new(pool.clone());
        let admin = sessions.create_session("adm", 3600).await.unwrap();
        let metrics = crate::observability::metrics_builder()
            .build_recorder()
            .handle();
        let router = crate::build_router(store, vec![7u8; 32], metrics)
            .await
            .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap()
        });
        Daemon {
            base,
            admin,
            pool,
            client: reqwest::Client::new(),
            _dir: dir,
        }
    }

    fn ids(list: &Value, key: &str) -> Vec<String> {
        let mut ids: Vec<String> = list[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["id"].as_str().unwrap().to_string())
            .collect();
        ids.sort();
        ids
    }

    #[tokio::test]
    async fn new_tokens_are_prefixed_and_default_to_read_and_build() {
        let d = daemon().await;
        let created = d.token(json!({"name": "ci", "role": "admin"})).await;
        assert!(created["token"].as_str().unwrap().starts_with("oore_"));
        assert!(created["prefix"].as_str().unwrap().starts_with("oore_"));
        assert_eq!(created["abilities"], json!(["read", "build"]));
        assert_eq!(created["project_ids"], json!([]));

        // `read` is always included; an empty list is refused.
        let chosen = d
            .token(json!({"name": "ops", "role": "admin", "abilities": ["release_operate"]}))
            .await;
        assert_eq!(chosen["abilities"], json!(["read", "release_operate"]));
        let (status, _) = d
            .send(
                "POST",
                &d.admin,
                "/v1/api-tokens",
                json!({"name": "none", "role": "admin", "abilities": []}),
            )
            .await;
        assert_eq!(status, 400);
        let (status, _) = d
            .send(
                "POST",
                &d.admin,
                "/v1/api-tokens",
                json!({"name": "past", "role": "admin", "expires_at": 1}),
            )
            .await;
        assert_eq!(status, 400);
        let (status, body) = d
            .send(
                "POST",
                &d.admin,
                "/v1/api-tokens",
                json!({"name": "ghost", "role": "admin", "project_ids": ["nope"]}),
            )
            .await;
        assert_eq!(status, 400, "{body}");
    }

    #[tokio::test]
    async fn old_tokens_keep_the_abilities_their_role_implies() {
        let d = daemon().await;
        let legacy = crate::token::generate_token();
        sqlx::query(
            "INSERT INTO api_tokens (id, name, token_hash, prefix, created_by, role, created_at) \
             VALUES ('old', 'old', ?1, ?2, 'adm', 'developer', 0)",
        )
        .bind(hash_token(&legacy))
        .bind(&legacy[..8])
        .execute(&d.pool)
        .await
        .unwrap();
        let (status, list) = d.get(&d.admin, "/v1/api-tokens").await;
        assert_eq!(status, 200);
        let old = &list["tokens"][0];
        assert_eq!(
            old["abilities"],
            json!(["read", "build", "release_operate", "manage"])
        );
        assert_eq!(old["abilities_chosen"], json!(false));
        // A developer token of an admin is capped at developer, as before.
        let (status, _) = d.get(&legacy, "/v1/projects").await;
        assert_eq!(status, 200);
        let (status, _) = d.get(&legacy, "/v1/users").await;
        assert_eq!(status, 403);
    }

    #[tokio::test]
    async fn a_token_limited_to_one_app_sees_and_touches_nothing_else() {
        let d = daemon().await;
        let created = d
            .token(json!({"name": "kite-agent", "role": "admin", "abilities": ["read", "build"], "project_ids": ["p1"]}))
            .await;
        let token = created["token"].as_str().unwrap();

        let (status, projects) = d.get(token, "/v1/projects").await;
        assert_eq!(status, 200);
        assert_eq!(ids(&projects, "projects"), ["p1"]);
        assert_eq!(projects["total"], json!(1));
        let (status, builds) = d.get(token, "/v1/builds").await;
        assert_eq!(status, 200);
        assert_eq!(ids(&builds, "builds"), ["b1"]);
        assert_eq!(d.get(token, "/v1/projects/p1").await.0, 200);
        assert_eq!(d.get(token, "/v1/projects/p2").await.0, 404);
        assert_eq!(d.get(token, "/v1/builds/b2").await.0, 404);
        assert_eq!(
            d.send("POST", token, "/v1/builds/b2/cancel", json!({}))
                .await
                .0,
            404
        );
        // The instance audit log spans every app.
        assert_eq!(d.get(token, "/v1/audit-logs").await.0, 403);

        // The admin's own session still sees both apps.
        let (_, projects) = d.get(&d.admin, "/v1/projects").await;
        assert_eq!(ids(&projects, "projects"), ["p1", "p2"]);
    }

    #[tokio::test]
    async fn abilities_limit_what_a_token_may_do() {
        let d = daemon().await;
        let read_only = d
            .token(json!({"name": "watcher", "role": "admin", "abilities": ["read"]}))
            .await;
        let read_only = read_only["token"].as_str().unwrap();
        assert_eq!(d.get(read_only, "/v1/builds/b1").await.0, 200);
        assert_eq!(d.get(read_only, "/v1/users").await.0, 200);
        assert_eq!(
            d.send("POST", read_only, "/v1/builds/b1/cancel", json!({}))
                .await
                .0,
            403
        );
        // Settings and token management need `manage`.
        assert_eq!(
            d.send("DELETE", read_only, "/v1/projects/p1", json!({}))
                .await
                .0,
            403
        );
        assert_eq!(
            d.send("DELETE", read_only, "/v1/api-tokens/x", json!({}))
                .await
                .0,
            403
        );
        assert_eq!(
            d.send(
                "POST",
                read_only,
                "/v1/api-tokens",
                json!({"name": "x", "role": "admin"})
            )
            .await
            .0,
            403
        );

        let builder = d
            .token(json!({"name": "builder", "role": "admin", "abilities": ["build"]}))
            .await;
        let builder = builder["token"].as_str().unwrap();
        let (status, body) = d
            .send("POST", builder, "/v1/builds/b1/cancel", json!({}))
            .await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            d.send("DELETE", builder, "/v1/projects/p1", json!({}))
                .await
                .0,
            403
        );
    }

    #[tokio::test]
    async fn a_token_cannot_make_a_broader_token_and_its_actions_are_recorded() {
        let d = daemon().await;
        let manager = d
            .token(json!({"name": "deploy-bot", "role": "admin", "abilities": ["read", "manage"], "project_ids": ["p1"]}))
            .await;
        let manager = manager["token"].as_str().unwrap();

        let (status, body) = d
            .send(
                "POST",
                manager,
                "/v1/api-tokens",
                json!({"name": "wider", "role": "admin", "abilities": ["build"]}),
            )
            .await;
        assert_eq!(status, 403, "{body}");
        assert_eq!(body["code"], json!("ability_escalation"));
        let (status, body) = d
            .send(
                "POST",
                manager,
                "/v1/api-tokens",
                json!({"name": "other-app", "role": "admin", "project_ids": ["p2"]}),
            )
            .await;
        assert_eq!(status, 403, "{body}");

        // Left out, the limits narrow to the caller's own.
        let (status, child) = d
            .send(
                "POST",
                manager,
                "/v1/api-tokens",
                json!({"name": "child", "role": "admin"}),
            )
            .await;
        assert_eq!(status, 200, "{child}");
        assert_eq!(child["abilities"], json!(["read"]));
        assert_eq!(child["project_ids"], json!(["p1"]));

        let (_, logs) = d.get(&d.admin, "/v1/audit-logs").await;
        let entry = logs["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["action"] == "api_token_created" && e["resource_id"] == child["id"])
            .unwrap();
        assert_eq!(entry["token_name"], json!("deploy-bot"));
        assert_eq!(entry["actor_email"], json!("admin@example.com"));

        // A build started by the admin session records no token.
        let (_, build) = d.get(&d.admin, "/v1/builds/b1").await;
        assert!(build.get("trigger_token_name").is_none());
    }

    #[tokio::test]
    async fn every_project_permission_needs_its_ability() {
        use Ability as A;
        use ProjectPermission as P;
        let table = [
            (P::Read, A::Read),
            (P::ReadArtifacts, A::Read),
            (P::TriggerBuild, A::Build),
            (P::CancelBuild, A::Build),
            (P::WriteArtifacts, A::Build),
            (P::OperateRelease, A::ReleaseOperate),
            (P::ManageReleases, A::ReleaseManage),
            (P::ApproveBuild, A::ReleaseManage),
            (P::Write, A::Manage),
            (P::Delete, A::Manage),
            (P::ManageMembers, A::Manage),
            (P::ManagePipelines, A::Manage),
            (P::DeleteArtifacts, A::Manage),
        ];
        for (permission, needed) in table {
            assert_eq!(permission.ability(), needed, "{permission:?}");
            for held in Ability::ALL {
                let grant = TokenGrant {
                    id: "t".into(),
                    name: "t".into(),
                    abilities: vec![held],
                    project_ids: vec![],
                    approval_abilities: vec![],
                };
                let allowed = crate::session::scope_request_grant(Some(Arc::new(grant)), async {
                    check_project_permission(&EffectiveProjectRole::InstanceAdmin, permission)
                })
                .await;
                assert_eq!(allowed, held == needed, "{permission:?} with {held:?}");
            }
            // Outside a token, the role alone decides.
            assert!(check_project_permission(
                &EffectiveProjectRole::Member(ProjectRole::Maintainer),
                permission
            ));
        }
        assert_eq!(rbac::ability_for("users", "read"), A::Read);
        assert_eq!(rbac::ability_for("builds", "cancel"), A::Build);
        assert_eq!(rbac::ability_for("artifacts", "write"), A::Build);
        assert_eq!(rbac::ability_for("pipelines", "write"), A::Manage);
        assert_eq!(rbac::ability_for("instance_settings", "write"), A::Manage);
        assert_eq!(rbac::ability_for("api_tokens", "delete"), A::Manage);
    }
}
