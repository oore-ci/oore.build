-- Responses kept for 24 hours so a request retried with the same
-- Idempotency-Key replays the first answer instead of acting twice.
CREATE TABLE idempotency_keys (
    -- SHA-256 of the caller's credential, method, path and key.
    scope_hash TEXT PRIMARY KEY NOT NULL,
    body_hash TEXT NOT NULL,
    -- NULL while the first request is still running.
    status INTEGER,
    content_type TEXT,
    response_body BLOB,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_idempotency_keys_created_at ON idempotency_keys(created_at);
