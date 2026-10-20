-- Per-pipeline store publishing settings. `config` is store-specific JSON
-- without credentials; credentials are referenced by secret name.

CREATE TABLE IF NOT EXISTS pipeline_publish_targets (
    id TEXT PRIMARY KEY,
    pipeline_id TEXT NOT NULL,
    store TEXT NOT NULL CHECK (store IN ('google_play', 'app_store')),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    config TEXT NOT NULL,
    created_by TEXT,
    updated_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (pipeline_id) REFERENCES pipelines(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (updated_by) REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE (pipeline_id, store)
);
