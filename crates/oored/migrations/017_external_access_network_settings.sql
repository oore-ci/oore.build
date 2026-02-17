-- Persist owner-managed External Access network settings.
-- This replaces manual env edits for OORE_PUBLIC_URL / OORE_CORS_ORIGINS.

CREATE TABLE IF NOT EXISTS external_access_network_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1) DEFAULT 1,
    public_url TEXT,
    allowed_origins_json TEXT NOT NULL DEFAULT '[]',
    updated_by TEXT REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
