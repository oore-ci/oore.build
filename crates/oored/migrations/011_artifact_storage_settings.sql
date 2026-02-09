-- Persist artifact storage backend settings (owner/admin managed).
-- Secrets are encrypted before storage.

CREATE TABLE IF NOT EXISTS artifact_storage_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1) DEFAULT 1,
    provider TEXT NOT NULL DEFAULT 'disabled' CHECK (provider IN ('disabled', 'local', 's3', 'r2')),

    -- Local backend
    local_base_dir TEXT,

    -- S3-compatible backend (S3 or R2)
    s3_bucket TEXT,
    s3_region TEXT,
    s3_endpoint TEXT,
    s3_access_key_encrypted TEXT,
    s3_secret_key_encrypted TEXT,

    updated_by TEXT REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
