-- The builds list sorts by creation time by default and filters by trigger,
-- actor and creation-time range. Without these, each page sorts or scans
-- every historical build.
CREATE INDEX IF NOT EXISTS idx_builds_created_at ON builds(created_at);
CREATE INDEX IF NOT EXISTS idx_builds_trigger ON builds(trigger_type, trigger_event, created_at);
CREATE INDEX IF NOT EXISTS idx_builds_trigger_actor ON builds(trigger_actor, created_at);
