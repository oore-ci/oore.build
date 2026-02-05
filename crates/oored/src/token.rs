use rand::RngCore;
use sha2::{Digest, Sha256};

/// Generate a cryptographically random token (32 random bytes => 64 hex chars).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
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
