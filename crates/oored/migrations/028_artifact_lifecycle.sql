ALTER TABLE artifacts ADD COLUMN state TEXT NOT NULL DEFAULT 'available'
    CHECK (state IN ('pending', 'available', 'failed'));
ALTER TABLE artifacts ADD COLUMN finalized_at INTEGER;
ALTER TABLE artifacts ADD COLUMN error_message TEXT;

UPDATE artifacts SET finalized_at = created_at WHERE state = 'available';

DROP INDEX IF EXISTS idx_artifacts_build_id_checksum_unique;
CREATE UNIQUE INDEX idx_artifacts_build_id_checksum_available
ON artifacts (build_id, checksum)
WHERE state = 'available' AND checksum IS NOT NULL AND trim(checksum) <> '';

CREATE INDEX idx_artifacts_pending_created
ON artifacts (created_at) WHERE state = 'pending';
