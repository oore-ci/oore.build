-- Extend notification_deliveries to support runner events.
-- Adds runner_id and event_category columns, and makes build_id nullable
-- since runner events don't reference a build.

-- SQLite doesn't support ALTER COLUMN, so we recreate the table.
PRAGMA foreign_keys=OFF;

CREATE TABLE notification_deliveries_new (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL REFERENCES notification_channels(id) ON DELETE CASCADE,
    build_id TEXT REFERENCES builds(id) ON DELETE CASCADE,
    runner_id TEXT,
    event_type TEXT NOT NULL,
    event_category TEXT NOT NULL DEFAULT 'build',
    status TEXT NOT NULL CHECK (status IN ('pending', 'delivered', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    delivered_at INTEGER
);

INSERT INTO notification_deliveries_new
    (id, channel_id, build_id, event_type, event_category, status, attempt_count, last_error, created_at, delivered_at)
SELECT id, channel_id, build_id, event_type, 'build', status, attempt_count, last_error, created_at, delivered_at
FROM notification_deliveries;

DROP TABLE notification_deliveries;
ALTER TABLE notification_deliveries_new RENAME TO notification_deliveries;

CREATE INDEX idx_notification_deliveries_channel ON notification_deliveries(channel_id);
CREATE INDEX idx_notification_deliveries_build ON notification_deliveries(build_id);
CREATE INDEX idx_notification_deliveries_status ON notification_deliveries(status);
CREATE INDEX idx_notification_deliveries_runner ON notification_deliveries(runner_id);

PRAGMA foreign_keys=ON;
