use sqlx::{Row, SqlitePool};

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

/// Information about a validated session, including the user's identity and role.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub user_id: String,
    pub email: String,
    pub oidc_subject: String,
    pub role: String,
    pub expires_at: i64,
    pub auth_source: AuthSource,
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
