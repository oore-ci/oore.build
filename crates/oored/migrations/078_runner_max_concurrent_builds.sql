-- How many builds a runner runs at the same time. One keeps the old behavior.
ALTER TABLE runners ADD COLUMN max_concurrent_builds INTEGER NOT NULL DEFAULT 1;

-- Counts a runner's builds against that limit on every claim. The status
-- list must stay identical to the claim queries in runners.rs.
CREATE INDEX IF NOT EXISTS idx_builds_runner_active
    ON builds (runner_id, status)
    WHERE status IN ('assigned', 'running');
