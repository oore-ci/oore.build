-- A dependency cache purge waiting for the runner's next heartbeat.
ALTER TABLE runners ADD COLUMN cache_purge_requested_at INTEGER;
