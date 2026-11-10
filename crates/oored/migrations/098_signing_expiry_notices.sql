-- Expiry reminders already sent, so each certificate or profile notifies
-- once at 30 days, once at 7 days and once when it expires. A replaced file
-- has a new expiry date, so its reminders start again.
CREATE TABLE IF NOT EXISTS signing_expiry_notices (
    credential_key TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    band TEXT NOT NULL CHECK (band IN ('month', 'week', 'expired')),
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    notified_at INTEGER NOT NULL,
    PRIMARY KEY (credential_key, expires_at, band)
);
