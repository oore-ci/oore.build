-- App Store Connect progress for releases with store = 'app_store'.
-- The config column snapshots the pipeline's App Store publish settings at
-- upload time so later edits do not change an in-flight release.
CREATE TABLE app_store_release_links (
    release_id TEXT PRIMARY KEY NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    bundle_id TEXT NOT NULL,
    version TEXT NOT NULL,
    build_number TEXT NOT NULL,
    config TEXT NOT NULL,
    stage TEXT NOT NULL DEFAULT 'awaiting_build'
        CHECK (stage IN ('awaiting_build', 'distribute', 'beta_review', 'review', 'live', 'done')),
    asc_app_id TEXT,
    asc_build_id TEXT,
    asc_version_id TEXT,
    last_error TEXT,
    next_poll_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_app_store_release_links_due
ON app_store_release_links (next_poll_at) WHERE stage <> 'done';
