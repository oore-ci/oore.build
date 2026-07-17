CREATE TABLE integration_repository_webhook_secrets (
    repository_id TEXT PRIMARY KEY NOT NULL,
    encrypted_secret TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (repository_id) REFERENCES integration_repositories(id) ON DELETE CASCADE
);

-- Integration-wide GitLab webhook credentials cannot be safely migrated to a
-- narrower scope. Retire them fail-closed; owners rotate a new token for each
-- repository from its source detail page.
DELETE FROM integration_credentials
WHERE credential_type = 'webhook_secret'
  AND integration_id IN (
      SELECT id FROM integrations WHERE provider = 'gitlab'
  );
