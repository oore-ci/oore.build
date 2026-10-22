-- App Store Connect API keys shared across pipelines of the instance.
-- The private key is encrypted at rest with the instance encryption key.

CREATE TABLE IF NOT EXISTS app_store_connect_keys (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    key_id TEXT NOT NULL,
    issuer_id TEXT NOT NULL,
    private_key_encrypted TEXT NOT NULL,
    created_by TEXT,
    updated_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (updated_by) REFERENCES users(id) ON DELETE SET NULL
);

-- When set, the pipeline signs and publishes with the shared key instead of
-- its own api_* columns, which stay stored for switching back.
ALTER TABLE pipeline_ios_signing_settings
ADD COLUMN shared_api_key_id TEXT REFERENCES app_store_connect_keys(id) ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_pipeline_ios_signing_settings_shared_key
ON pipeline_ios_signing_settings(shared_api_key_id);
