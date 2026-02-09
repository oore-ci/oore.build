CREATE TABLE IF NOT EXISTS instance_preferences (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  key_storage_mode TEXT NOT NULL DEFAULT 'keychain',
  updated_by TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
