-- Per-pipeline build timeout and approval gate, build approval state, and
-- cron schedule checkpoints. Tag, path and schedule triggers live in the
-- existing pipelines.trigger_config JSON.

ALTER TABLE pipelines ADD COLUMN timeout_minutes INTEGER NOT NULL DEFAULT 60;
ALTER TABLE pipelines ADD COLUMN approval_policy TEXT NOT NULL DEFAULT '{}';

-- A gated build stays 'queued' with approval_state 'pending'; runners skip it
-- until a maintainer approves. Rejecting cancels it.
ALTER TABLE builds ADD COLUMN approval_state TEXT
    CHECK (approval_state IN ('pending', 'approved', 'rejected'));
ALTER TABLE builds ADD COLUMN approval_requested_at INTEGER;
ALTER TABLE builds ADD COLUMN approval_decided_by TEXT;
ALTER TABLE builds ADD COLUMN approval_decided_at INTEGER;
ALTER TABLE builds ADD COLUMN approval_reason TEXT;

CREATE INDEX IF NOT EXISTS idx_builds_pending_approval
    ON builds(project_id) WHERE approval_state = 'pending';

CREATE TABLE IF NOT EXISTS pipeline_schedule_runs (
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    -- cron expression and branch, so editing a schedule starts it afresh
    schedule_key TEXT NOT NULL,
    last_checked_at INTEGER NOT NULL,
    PRIMARY KEY (pipeline_id, schedule_key)
);
