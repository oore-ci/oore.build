-- OOR-140: Artifact expiry and scoped download tokens

-- Add per-artifact expiry timestamp
ALTER TABLE artifacts ADD COLUMN expires_at INTEGER;
CREATE INDEX IF NOT EXISTS idx_artifacts_expires_at ON artifacts(expires_at)
    WHERE expires_at IS NOT NULL;

-- Scoped download tokens (DB-backed, shareable links)
CREATE TABLE IF NOT EXISTS artifact_download_tokens (
    id           TEXT PRIMARY KEY,
    artifact_id  TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    token_hash   TEXT NOT NULL UNIQUE,
    prefix       TEXT NOT NULL,
    created_by   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at   INTEGER NOT NULL,
    single_use   INTEGER NOT NULL DEFAULT 0,
    used_at      INTEGER,
    revoked_at   INTEGER,
    created_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_artifact_dl_tokens_hash ON artifact_download_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_artifact_dl_tokens_artifact ON artifact_download_tokens(artifact_id);
CREATE INDEX IF NOT EXISTS idx_artifact_dl_tokens_expires ON artifact_download_tokens(expires_at);

-- Add default artifact TTL to retention policy
ALTER TABLE retention_policy ADD COLUMN artifact_ttl_days INTEGER;

-- Add per-project artifact TTL override
ALTER TABLE project_retention_overrides ADD COLUMN artifact_ttl_days INTEGER;
