CREATE INDEX IF NOT EXISTS idx_builds_claim_queue
    ON builds (status, queued_at, id);
