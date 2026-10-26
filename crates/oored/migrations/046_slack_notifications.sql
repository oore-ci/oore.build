-- Slack notification channels, per-project channel scope, and release events.
-- SQLite doesn't support ALTER CHECK, so notification_channels is recreated.

PRAGMA foreign_keys=OFF;

CREATE TABLE notification_channels_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    channel_type TEXT NOT NULL CHECK (channel_type IN ('webhook', 'mattermost', 'email', 'slack')),
    enabled INTEGER NOT NULL DEFAULT 1,
    -- JSON array of event strings to filter on; NULL means all events
    event_filter_json TEXT,
    -- NULL means instance-wide; otherwise only events for this project
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    -- Webhook/Mattermost/Slack: AES-256-GCM encrypted URL (base64)
    encrypted_url TEXT,
    -- Webhook: encrypted HMAC secret. Slack: encrypted bot token for mentions.
    encrypted_secret TEXT,
    -- Email: AES-256-GCM encrypted JSON blob with SMTP config
    encrypted_config TEXT,
    created_by TEXT REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO notification_channels_new
    (id, name, channel_type, enabled, event_filter_json, encrypted_url, encrypted_secret, encrypted_config, created_by, created_at, updated_at)
SELECT id, name, channel_type, enabled, event_filter_json, encrypted_url, encrypted_secret, encrypted_config, created_by, created_at, updated_at
FROM notification_channels;

DROP TABLE notification_channels;
ALTER TABLE notification_channels_new RENAME TO notification_channels;

CREATE INDEX idx_notification_channels_project ON notification_channels(project_id);

PRAGMA foreign_keys=ON;

ALTER TABLE notification_deliveries ADD COLUMN release_id TEXT;
CREATE INDEX idx_notification_deliveries_release ON notification_deliveries(release_id);
