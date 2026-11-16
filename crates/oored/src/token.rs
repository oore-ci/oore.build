use rand::RngCore;
use sha2::{Digest, Sha256};

/// Prefix on every API token made since v0.1.54, so secret scanners and
/// people can tell an Oore token apart from other random strings.
pub const API_TOKEN_PREFIX: &str = "oore_";

/// How many characters of a token are kept in clear as its display prefix:
/// `oore_` plus eight random characters for new tokens.
pub const API_TOKEN_DISPLAY_PREFIX_LEN: usize = API_TOKEN_PREFIX.len() + 8;

/// Expiry the web UI and CLI suggest for a new API token.
pub const DEFAULT_API_TOKEN_TTL_SECS: i64 = 90 * 86_400;

/// Generate a cryptographically random token (32 random bytes => 64 hex chars).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Generate a new API token: `oore_` followed by 64 random hex characters.
pub fn generate_api_token() -> String {
    format!("{API_TOKEN_PREFIX}{}", generate_token())
}

/// The part of an API token that may be shown again after creation.
pub fn api_token_display_prefix(token: &str) -> String {
    let len = if token.starts_with(API_TOKEN_PREFIX) {
        API_TOKEN_DISPLAY_PREFIX_LEN
    } else {
        8
    };
    token.chars().take(len).collect()
}

/// SHA-256 hash a token string, returning the hex-encoded digest.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a session token (identical to generate_token).
pub fn generate_session_token() -> String {
    generate_token()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_tokens_are_recognisable_and_keep_a_short_prefix() {
        let token = generate_api_token();
        assert!(token.starts_with("oore_"));
        assert_eq!(token.len(), 5 + 64);
        assert_eq!(api_token_display_prefix(&token), token[..13]);
        let legacy = generate_token();
        assert_eq!(api_token_display_prefix(&legacy), legacy[..8]);
        assert!(!generate_session_token().starts_with("oore_"));
    }
}
