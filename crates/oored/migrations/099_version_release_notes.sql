-- Release notes belong to an app version (such as 2.5.1), not to a pipeline
-- or a single build. Every build of the version shows them, and every store
-- upload and promotion of the version sends them.
CREATE TABLE IF NOT EXISTS version_release_notes (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    version_name TEXT NOT NULL,
    text TEXT NOT NULL DEFAULT '',
    -- Other languages, as a JSON array of {"locale", "text"}.
    locales TEXT NOT NULL DEFAULT '[]',
    source TEXT NOT NULL DEFAULT 'commits'
        CHECK (source IN ('commits', 'manual', 'repository')),
    source_path TEXT,
    drafted_since TEXT,
    updated_at INTEGER NOT NULL,
    updated_by TEXT,
    PRIMARY KEY (project_id, version_name)
);

-- Notes written for single builds become their version's notes: the newest
-- build with notes wins.
INSERT OR IGNORE INTO version_release_notes
    (project_id, version_name, text, source, updated_at)
SELECT project_id, version_name, store_notes, 'manual', updated_at
FROM (
    SELECT project_id, version_name, TRIM(store_notes) AS store_notes, updated_at,
           ROW_NUMBER() OVER (
               PARTITION BY project_id, version_name
               ORDER BY build_number DESC
           ) AS newest
    FROM builds
    WHERE version_name IS NOT NULL AND TRIM(version_name) != ''
      AND store_notes IS NOT NULL AND TRIM(store_notes) != ''
)
WHERE newest = 1;
