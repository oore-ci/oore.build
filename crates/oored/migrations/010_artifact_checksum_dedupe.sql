-- Ensure artifacts are unique by checksum within a build when checksum is present.
-- This prevents duplicate attachments for the same artifact content.

DELETE FROM artifacts
WHERE id IN (
    SELECT newer.id
    FROM artifacts AS newer
    JOIN artifacts AS older
      ON newer.build_id = older.build_id
     AND newer.checksum = older.checksum
     AND newer.checksum IS NOT NULL
     AND trim(newer.checksum) <> ''
     AND (
         newer.created_at > older.created_at
         OR (newer.created_at = older.created_at AND newer.rowid > older.rowid)
     )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_artifacts_build_id_checksum_unique
ON artifacts (build_id, checksum)
WHERE checksum IS NOT NULL AND trim(checksum) <> '';
