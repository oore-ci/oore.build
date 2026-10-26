-- The builds list sorts by creation time (then id) by default and filters by
-- trigger, actor and creation-time range. Without these, each page sorts or
-- scans every historical build.
CREATE INDEX IF NOT EXISTS idx_builds_created_at ON builds(created_at, id);
-- Trigger-type filters (manual, schedule) read in page order.
CREATE INDEX IF NOT EXISTS idx_builds_trigger_type ON builds(trigger_type, created_at, id);
-- Webhook event filters.
CREATE INDEX IF NOT EXISTS idx_builds_trigger_event ON builds(trigger_type, trigger_event, created_at);
CREATE INDEX IF NOT EXISTS idx_builds_trigger_actor ON builds(trigger_actor, created_at, id);
