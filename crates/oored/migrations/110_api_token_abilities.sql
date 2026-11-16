-- Narrower API tokens (T2): a list of abilities and a list of apps.
-- NULL abilities means a token made before abilities existed; its role
-- implies them. NULL or '[]' project_ids means every app the creator reaches.
ALTER TABLE api_tokens ADD COLUMN abilities TEXT;
ALTER TABLE api_tokens ADD COLUMN project_ids TEXT;

-- Say when a token acted (T3).
ALTER TABLE audit_logs ADD COLUMN token_id TEXT;
ALTER TABLE audit_logs ADD COLUMN token_name TEXT;
ALTER TABLE builds ADD COLUMN trigger_token_id TEXT;
ALTER TABLE builds ADD COLUMN trigger_token_name TEXT;
