ALTER TABLE instance_preferences
  ADD COLUMN remote_auth_mode TEXT NOT NULL DEFAULT 'oidc'
  CHECK (remote_auth_mode IN ('oidc', 'trusted_proxy'));

UPDATE instance_preferences
SET remote_auth_mode = 'oidc'
WHERE remote_auth_mode IS NULL OR remote_auth_mode = '';

CREATE TABLE IF NOT EXISTS trusted_proxy_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1) DEFAULT 1,
    user_email_header TEXT NOT NULL DEFAULT 'x-warpgate-username',
    trusted_proxy_cidrs_json TEXT NOT NULL DEFAULT '[]',
    encrypted_shared_secret TEXT,
    updated_by TEXT REFERENCES users(id),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
