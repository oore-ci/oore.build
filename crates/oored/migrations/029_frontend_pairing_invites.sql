CREATE TABLE IF NOT EXISTS frontend_pairing_invites (
    id TEXT PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_frontend_pairing_invites_active
    ON frontend_pairing_invites (expires_at, consumed_at);
