ALTER TABLE builds ADD COLUMN source_build_id TEXT REFERENCES builds(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_builds_source ON builds(source_build_id);
