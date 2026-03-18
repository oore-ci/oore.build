-- Notification channels for outbound build status notifications.
-- Supports webhook (generic HTTP POST) and Mattermost (Slack-compatible incoming webhook).
-- Email channel type is reserved for future use (OOR-144).

CREATE TABLE IF NOT EXISTS notification_channels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    -- 'webhook' or 'mattermost' (extensible for future 'email')
    channel_type TEXT NOT NULL CHECK (channel_type IN ('webhook', 'mattermost')),
    enabled INTEGER NOT NULL DEFAULT 1,
    -- JSON array of build status strings to filter on, e.g. ["succeeded","failed"]
    -- NULL means all terminal events
    event_filter_json TEXT,
    -- Webhook/Mattermost: AES-256-GCM encrypted URL (base64)
    encrypted_url TEXT,
    -- Webhook only: optional AES-256-GCM encrypted HMAC secret for payload signing
    encrypted_secret TEXT,
    created_by TEXT REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS notification_deliveries (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL REFERENCES notification_channels(id) ON DELETE CASCADE,
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'delivered', 'failed')),
    attempt_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    created_at INTEGER NOT NULL,
    delivered_at INTEGER
);

CREATE INDEX idx_notification_deliveries_channel ON notification_deliveries(channel_id);
CREATE INDEX idx_notification_deliveries_build ON notification_deliveries(build_id);
CREATE INDEX idx_notification_deliveries_status ON notification_deliveries(status);
