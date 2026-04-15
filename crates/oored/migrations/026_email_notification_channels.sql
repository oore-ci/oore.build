-- Add email as a notification channel type (OOR-144).
-- Adds encrypted_config column for SMTP configuration storage.
-- SQLite doesn't support ALTER CHECK, so we recreate the table.

PRAGMA foreign_keys=OFF;

CREATE TABLE notification_channels_new (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    -- 'webhook', 'mattermost', or 'email'
    channel_type TEXT NOT NULL CHECK (channel_type IN ('webhook', 'mattermost', 'email')),
    enabled INTEGER NOT NULL DEFAULT 1,
    -- JSON array of event strings to filter on; NULL means all terminal events
    event_filter_json TEXT,
    -- Webhook/Mattermost: AES-256-GCM encrypted URL (base64)
    encrypted_url TEXT,
    -- Webhook only: optional AES-256-GCM encrypted HMAC secret for payload signing
    encrypted_secret TEXT,
    -- Email: AES-256-GCM encrypted JSON blob with SMTP config (host, port, credentials, recipients)
    encrypted_config TEXT,
    created_by TEXT REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO notification_channels_new
    (id, name, channel_type, enabled, event_filter_json, encrypted_url, encrypted_secret, created_by, created_at, updated_at)
SELECT id, name, channel_type, enabled, event_filter_json, encrypted_url, encrypted_secret, created_by, created_at, updated_at
FROM notification_channels;

DROP TABLE notification_channels;
ALTER TABLE notification_channels_new RENAME TO notification_channels;

PRAGMA foreign_keys=ON;
