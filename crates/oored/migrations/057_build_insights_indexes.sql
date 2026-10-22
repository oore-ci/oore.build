-- Home build insights read builds by when they finished and started. Without
-- these, the window filters scan every historical build.
CREATE INDEX IF NOT EXISTS idx_builds_finished_at ON builds(finished_at);
CREATE INDEX IF NOT EXISTS idx_builds_started_at ON builds(started_at);
