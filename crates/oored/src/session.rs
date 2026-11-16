use std::future::Future;
use std::sync::Arc;

use sqlx::{Row, SqlitePool};

pub use oore_contract::TokenAbility as Ability;

use crate::token::{generate_session_token, hash_token};
use crate::util::now_unix;

/// Default session time-to-live: 24 hours in seconds.
pub const DEFAULT_SESSION_TTL: i64 = 86400;

/// How the current request was authenticated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSource {
    Session,
    ApiToken,
}

/// What an API token was limited to when it was made.
///
/// A token may do the least of: its creator's own rights, its role, its
/// abilities and its app list. The role and the creator's rights are applied
/// through `SessionInfo::role` and project membership as before; this grant
/// carries the other two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenGrant {
    pub id: String,
    pub name: String,
    /// Abilities the token holds. Tokens made before abilities existed get
    /// the abilities their role implies.
    pub abilities: Vec<Ability>,
    /// Apps the token may reach. Empty means every app its creator reaches.
    pub project_ids: Vec<String>,
    /// Abilities whose calls wait for a person to approve them (T7).
    pub approval_abilities: Vec<Ability>,
}

impl TokenGrant {
    pub fn has_ability(&self, ability: Ability) -> bool {
        self.abilities.contains(&ability)
    }

    pub fn allows_project(&self, project_id: &str) -> bool {
        self.project_ids.is_empty() || self.project_ids.iter().any(|id| id == project_id)
    }
}

/// Information about a validated session, including the user's identity and role.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub user_id: String,
    pub email: String,
    pub oidc_subject: String,
    pub role: String,
    pub expires_at: i64,
    pub auth_source: AuthSource,
    /// Set when the request was signed in with an API token.
    pub token: Option<Arc<TokenGrant>>,
}

impl SessionInfo {
    /// Whether this session holds `ability`. A sign-in session holds every
    /// ability; its role still limits it through the usual permission checks.
    pub fn has_ability(&self, ability: Ability) -> bool {
        self.token
            .as_ref()
            .is_none_or(|grant| grant.has_ability(ability))
    }

    /// Whether this session may reach `project_id` at all, before roles.
    pub fn allows_project(&self, project_id: &str) -> bool {
        self.token
            .as_ref()
            .is_none_or(|grant| grant.allows_project(project_id))
    }

    pub fn token_id(&self) -> Option<&str> {
        self.token.as_deref().map(|grant| grant.id.as_str())
    }

    pub fn token_name(&self) -> Option<&str> {
        self.token.as_deref().map(|grant| grant.name.as_str())
    }

    /// What stream and rate limits count against: the API token when the
    /// caller used one, so a person's agents cannot crowd out their own
    /// browser, and the person otherwise. A per-person total still applies.
    pub fn limit_key(&self) -> String {
        match self.token_id() {
            Some(token_id) => format!("token:{token_id}"),
            None => format!("user:{}", self.user_id),
        }
    }

    /// The abilities this session holds, in their fixed order.
    pub fn abilities(&self) -> Vec<Ability> {
        match &self.token {
            Some(grant) => grant.abilities.clone(),
            None => Ability::ALL.to_vec(),
        }
    }
}

// ── Per-request token grant ─────────────────────────────────────
//
// The permission helpers (`rbac::check_permission`, `project_rbac`) and the
// audit writer take a role or a pool, not the session, and have well over a
// hundred callers. Rather than thread the token through each one, a router
// layer (`extractors::scope_request_auth`) runs every request inside a
// task-local holding the caller's token grant, and those helpers read it.
// Code outside a request (background jobs) has no grant and is unaffected.

#[derive(Debug, Clone)]
struct RequestGrant(Option<Arc<TokenGrant>>);

tokio::task_local! {
    static REQUEST_GRANT: RequestGrant;
}

/// Run `fut` with `grant` as the current request's token grant.
pub async fn scope_request_grant<F: Future>(grant: Option<Arc<TokenGrant>>, fut: F) -> F::Output {
    REQUEST_GRANT.scope(RequestGrant(grant), fut).await
}

/// Whether the caller runs inside a request scope set by the auth layer.
pub fn in_request_scope() -> bool {
    REQUEST_GRANT.try_with(|_| ()).is_ok()
}

/// The token grant of the request being served, if it used an API token.
pub fn current_grant() -> Option<Arc<TokenGrant>> {
    REQUEST_GRANT.try_with(|g| g.0.clone()).ok().flatten()
}

/// Whether the current request may use `ability`. True outside API tokens.
pub fn current_has_ability(ability: Ability) -> bool {
    current_grant().is_none_or(|grant| grant.has_ability(ability))
}

/// Whether the current request may reach `project_id`. True outside API
/// tokens and for tokens without an app list.
pub fn current_allows_project(project_id: &str) -> bool {
    current_grant().is_none_or(|grant| grant.allows_project(project_id))
}

/// A SQL condition that keeps only the apps the current request's token may
/// reach, such as `p.id IN ('a','b')`, or `None` when there is no app list.
/// Use it in list and search queries next to the role-based filter.
pub fn current_project_limit_sql(column: &str) -> Option<String> {
    current_project_limit().map(|ids| {
        let quoted = ids
            .iter()
            .map(|id| format!("'{}'", id.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{column} IN ({quoted})")
    })
}

/// The app list of the current request's token, when it has one.
pub fn current_project_limit() -> Option<Vec<String>> {
    current_grant()
        .filter(|grant| !grant.project_ids.is_empty())
        .map(|grant| grant.project_ids.clone())
}

/// SQLite-backed session store.
///
/// Session tokens are hashed (SHA-256) before storage. The plaintext token is
/// returned to the caller on creation and never stored.
pub struct SessionStore {
    pool: SqlitePool,
}

impl SessionStore {
    /// Create a new session store backed by the given SQLite pool.
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Create a new session for the given user.
    ///
    /// Generates a random token, hashes it, stores the session keyed by hash,
    /// and returns the plaintext token for the caller to send to the client.
    pub async fn create_session(
        &self,
        user_id: &str,
        ttl_secs: i64,
    ) -> Result<String, sqlx::Error> {
        let token = generate_session_token();
        let hashed = hash_token(&token);
        let now = now_unix();
        let expires_at = now + ttl_secs;

        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(&hashed)
        .bind(user_id)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(token)
    }

    /// Validate a session token and return the associated session info if it
    /// exists and has not expired. Joins with the `users` table to include
    /// user identity and role.
    pub async fn validate_session(&self, token: &str) -> Result<Option<SessionInfo>, sqlx::Error> {
        let hashed = hash_token(token);
        let now = now_unix();

        let row = sqlx::query(
            "SELECT u.id, u.email, u.oidc_subject, u.role, s.expires_at \
             FROM sessions s \
             JOIN users u ON u.id = s.user_id \
             WHERE s.token_hash = ?1 AND s.expires_at > ?2 AND u.status = 'active'",
        )
        .bind(&hashed)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| SessionInfo {
            user_id: r.get("id"),
            email: r.get("email"),
            oidc_subject: r.get("oidc_subject"),
            role: r.get("role"),
            expires_at: r.get("expires_at"),
            auth_source: AuthSource::Session,
            token: None,
        }))
    }

    /// Revoke (remove) a session by its plaintext token.
    ///
    /// Returns `true` if the session existed and was removed.
    pub async fn revoke_session(&self, token: &str) -> Result<bool, sqlx::Error> {
        let hashed = hash_token(token);
        let result = sqlx::query("DELETE FROM sessions WHERE token_hash = ?1")
            .bind(&hashed)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Revoke all sessions for a given user.
    pub async fn revoke_user_sessions(&self, user_id: &str) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM sessions WHERE user_id = ?1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Revoke every active session in the instance.
    pub async fn revoke_all_sessions(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM sessions")
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Remove all expired sessions from the store.
    pub async fn cleanup_expired(&self) -> Result<u64, sqlx::Error> {
        let now = now_unix();
        let result = sqlx::query("DELETE FROM sessions WHERE expires_at <= ?1")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}
