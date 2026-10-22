-- Why a source is in its current status, so the UI can explain an error
-- instead of showing a bare badge.
ALTER TABLE integrations ADD COLUMN status_reason TEXT;
ALTER TABLE integrations ADD COLUMN status_checked_at INTEGER;
