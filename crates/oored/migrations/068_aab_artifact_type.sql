-- no-transaction
-- Allow 'aab' (Android App Bundle) as an artifact type.
-- SQLite can't alter a CHECK constraint, so artifacts is recreated. Foreign
-- keys must be off while the old table is dropped, or the implicit delete
-- would cascade to artifact_download_tokens and null releases.artifact_id.
-- PRAGMA foreign_keys is a no-op inside a transaction, so this migration
-- manages its own.

PRAGMA foreign_keys=OFF;

BEGIN;

CREATE TABLE artifacts_new (
    id TEXT PRIMARY KEY NOT NULL,
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    artifact_type TEXT NOT NULL DEFAULT 'generic' CHECK (artifact_type IN ('apk', 'ipa', 'app', 'aab', 'generic')),
    file_path TEXT NOT NULL,  -- S3 key or local path
    file_size INTEGER,
    checksum TEXT,            -- SHA-256 hash
    metadata TEXT NOT NULL DEFAULT '{}',  -- JSON: additional metadata
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    state TEXT NOT NULL DEFAULT 'available'
        CHECK (state IN ('pending', 'available', 'failed')),
    finalized_at INTEGER,
    error_message TEXT
);

INSERT INTO artifacts_new
    (id, build_id, name, artifact_type, file_path, file_size, checksum, metadata, created_at, expires_at, state, finalized_at, error_message)
SELECT id, build_id, name, artifact_type, file_path, file_size, checksum, metadata, created_at, expires_at, state, finalized_at, error_message
FROM artifacts;

DROP TABLE artifacts;
ALTER TABLE artifacts_new RENAME TO artifacts;

CREATE INDEX idx_artifacts_build ON artifacts(build_id);
CREATE INDEX idx_artifacts_expires_at ON artifacts(expires_at)
    WHERE expires_at IS NOT NULL;
CREATE UNIQUE INDEX idx_artifacts_build_id_checksum_available
ON artifacts (build_id, checksum)
WHERE state = 'available' AND checksum IS NOT NULL AND trim(checksum) <> '';
CREATE INDEX idx_artifacts_pending_created
ON artifacts (created_at) WHERE state = 'pending';

COMMIT;

PRAGMA foreign_keys=ON;
