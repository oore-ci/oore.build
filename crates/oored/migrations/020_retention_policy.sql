-- Build retention and cleanup policy settings (OOR-137).
--
-- Global singleton table stores the instance-wide default policy.
-- Per-project overrides allow customizing retention per project;
-- NULL fields inherit from the global policy.

-- Global retention policy (singleton, id=1)
CREATE TABLE IF NOT EXISTS retention_policy (
    id INTEGER PRIMARY KEY CHECK (id = 1) DEFAULT 1,
    enabled INTEGER NOT NULL DEFAULT 0,
    max_age_days INTEGER,
    max_builds_per_project INTEGER,
    max_artifact_size_bytes INTEGER,
    cleanup_target TEXT NOT NULL DEFAULT 'artifacts_only'
        CHECK (cleanup_target IN ('artifacts_only', 'full')),
    keep_statuses TEXT NOT NULL DEFAULT '[]',
    dry_run INTEGER NOT NULL DEFAULT 0,
    cleanup_interval_secs INTEGER NOT NULL DEFAULT 3600,
    updated_by TEXT REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Per-project retention overrides (NULL = inherit from global)
CREATE TABLE IF NOT EXISTS project_retention_overrides (
    project_id TEXT PRIMARY KEY NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    enabled INTEGER,
    max_age_days INTEGER,
    max_builds_per_project INTEGER,
    max_artifact_size_bytes INTEGER,
    cleanup_target TEXT CHECK (cleanup_target IS NULL OR cleanup_target IN ('artifacts_only', 'full')),
    keep_statuses TEXT,
    updated_by TEXT REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
