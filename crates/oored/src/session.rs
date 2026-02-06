use std::collections::HashMap;

use crate::token::{generate_session_token, hash_token};
use crate::util::now_unix;

/// Default session time-to-live: 24 hours in seconds.
pub const DEFAULT_SESSION_TTL: i64 = 86400;

/// Represents an authenticated user session.
#[derive(Debug, Clone)]
pub struct Session {
    pub user_email: String,
    pub oidc_subject: String,
    pub created_at: i64,
    pub expires_at: i64,
}

/// In-memory session store backed by a HashMap.
///
/// Session tokens are hashed before storage using the same `hash_token`
/// function from the token module for consistency with the rest of the
/// codebase. The plaintext token is returned to the caller on creation
/// and never stored.
pub struct SessionStore {
    sessions: HashMap<String, Session>,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStore {
    /// Create a new empty session store.
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Create a new session for the given user.
    ///
    /// Generates a random token, hashes it, stores the session keyed by hash,
    /// and returns the plaintext token for the caller to send to the client.
    pub fn create_session(&mut self, email: &str, oidc_subject: &str, ttl_secs: i64) -> String {
        let token = generate_session_token();
        let hashed = hash_token(&token);
        let now = now_unix();

        let session = Session {
            user_email: email.to_string(),
            oidc_subject: oidc_subject.to_string(),
            created_at: now,
            expires_at: now + ttl_secs,
        };

        self.sessions.insert(hashed, session);
        token
    }

    /// Validate a session token and return the associated session if it
    /// exists and has not expired.
    pub fn validate_session(&self, token: &str) -> Option<&Session> {
        let hashed = hash_token(token);
        let session = self.sessions.get(&hashed)?;

        if now_unix() > session.expires_at {
            return None;
        }

        Some(session)
    }

    /// Revoke (remove) a session by its plaintext token.
    ///
    /// Returns `true` if the session existed and was removed.
    pub fn revoke_session(&mut self, token: &str) -> bool {
        let hashed = hash_token(token);
        self.sessions.remove(&hashed).is_some()
    }

    /// Remove all expired sessions from the store.
    pub fn cleanup_expired(&mut self) {
        let now = now_unix();
        self.sessions.retain(|_, session| session.expires_at > now);
    }
}
