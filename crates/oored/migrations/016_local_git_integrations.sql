-- Extend integrations provider/auth_mode constraints for local_git support.
-- SQLite requires table recreation to update CHECK constraints.

PRAGMA foreign_keys=OFF;

CREATE TABLE integrations_new (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('github', 'gitlab', 'local_git')),
    host_url TEXT NOT NULL DEFAULT 'https://github.com',
    auth_mode TEXT NOT NULL CHECK (auth_mode IN ('github_app', 'oauth_app', 'personal_token', 'local_path')),
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive', 'error')),
    display_name TEXT,
    app_id TEXT,
    app_slug TEXT,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO integrations_new (
    id,
    provider,
    host_url,
    auth_mode,
    status,
    display_name,
    app_id,
    app_slug,
    created_by,
    created_at,
    updated_at
)
SELECT
    id,
    provider,
    host_url,
    auth_mode,
    status,
    display_name,
    app_id,
    app_slug,
    created_by,
    created_at,
    updated_at
FROM integrations;

DROP TABLE integrations;
ALTER TABLE integrations_new RENAME TO integrations;

CREATE INDEX IF NOT EXISTS idx_integrations_provider ON integrations(provider);
CREATE INDEX IF NOT EXISTS idx_integrations_created_by ON integrations(created_by);

PRAGMA foreign_keys=ON;
