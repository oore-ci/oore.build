-- The live event watcher scans recently changed builds and releases.
CREATE INDEX IF NOT EXISTS idx_builds_updated_at ON builds(updated_at);
CREATE INDEX IF NOT EXISTS idx_releases_updated_at ON releases(updated_at);
