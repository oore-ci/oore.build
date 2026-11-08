-- no-transaction
-- Release lifecycle (v0.1.51):
-- * new release states blocked, approved and rejected;
-- * releases made outside Oore: build_id becomes nullable, `origin` says
--   who made the release and `pipeline_id` names the pipeline whose store
--   credentials reach it;
-- * `auto_rollout` says whether Oore drives a Play rollout's schedule.
--
-- SQLite can't alter a CHECK or a NOT NULL in place, so releases is copied
-- into a new table. Every column is copied as it is; no value is changed.
-- Existing rows get origin 'oore', no pipeline_id (their build names it) and
-- auto_rollout 1, which is how Oore treated them before.
--
-- Foreign keys must be off while the old table is dropped, or the implicit
-- delete would cascade to release_history, app_store_release_links and
-- environment promotions. PRAGMA foreign_keys is a no-op inside a
-- transaction, so this migration manages its own. The row count is checked
-- before the old table is dropped; a mismatch aborts the migration.

PRAGMA foreign_keys=OFF;

BEGIN;

CREATE TABLE releases_new (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    build_id TEXT REFERENCES builds(id) ON DELETE CASCADE,
    artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    store TEXT NOT NULL CHECK (store IN ('google_play', 'app_store')),
    track TEXT NOT NULL,
    package_name TEXT,
    version_name TEXT,
    version_code INTEGER,
    state TEXT NOT NULL CHECK (state IN (
        'uploading', 'processing', 'blocked', 'in_review', 'approved',
        'rejected', 'rolling', 'paused', 'halted', 'complete', 'failed'
    )),
    rollout_fraction REAL NOT NULL DEFAULT 0 CHECK (rollout_fraction >= 0 AND rollout_fraction <= 1),
    next_step_at INTEGER,
    policy_json TEXT NOT NULL DEFAULT '{}',
    crash_rate REAL,
    crash_rate_checked_at INTEGER,
    approved_at INTEGER,
    approved_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    store_checked_at INTEGER,
    health_error TEXT,
    environment_id TEXT REFERENCES project_environments(id) ON DELETE SET NULL,
    promoted_from_release_id TEXT REFERENCES releases(id) ON DELETE SET NULL,
    origin TEXT NOT NULL DEFAULT 'oore' CHECK (origin IN ('oore', 'adopted')),
    pipeline_id TEXT REFERENCES pipelines(id) ON DELETE SET NULL,
    auto_rollout INTEGER NOT NULL DEFAULT 1 CHECK (auto_rollout IN (0, 1)),
    -- A release Oore uploaded or found has a build, or names the pipeline
    -- whose credentials reach it.
    CHECK (build_id IS NOT NULL OR origin = 'adopted')
);

INSERT INTO releases_new (
    id, project_id, build_id, artifact_id, store, track, package_name,
    version_name, version_code, state, rollout_fraction, next_step_at,
    policy_json, crash_rate, crash_rate_checked_at, approved_at, approved_by,
    created_by, created_at, updated_at, store_checked_at, health_error,
    environment_id, promoted_from_release_id
)
SELECT
    id, project_id, build_id, artifact_id, store, track, package_name,
    version_name, version_code, state, rollout_fraction, next_step_at,
    policy_json, crash_rate, crash_rate_checked_at, approved_at, approved_by,
    created_by, created_at, updated_at, store_checked_at, health_error,
    environment_id, promoted_from_release_id
FROM releases;

-- Aborts the migration unless every row was copied.
CREATE TEMP TABLE release_copy_check (
    copied INTEGER NOT NULL,
    original INTEGER NOT NULL,
    CHECK (copied = original)
);
INSERT INTO release_copy_check (copied, original)
SELECT (SELECT COUNT(*) FROM releases_new), (SELECT COUNT(*) FROM releases);
DROP TABLE release_copy_check;

DROP TABLE releases;
ALTER TABLE releases_new RENAME TO releases;

CREATE INDEX idx_releases_project ON releases (project_id, created_at DESC);
CREATE INDEX idx_releases_build ON releases (build_id);
CREATE INDEX idx_releases_due ON releases (state, next_step_at);
CREATE INDEX idx_releases_updated_at ON releases (updated_at);
CREATE INDEX idx_releases_environment ON releases (environment_id, created_at DESC);
CREATE INDEX idx_releases_store_version ON releases (project_id, store, package_name, version_code);

-- The store refusal a notification already went out for, so a release that
-- stays blocked for the same reason notifies once.
ALTER TABLE app_store_release_links ADD COLUMN notified_error TEXT;

-- When the monitor last read a release with the v0.1.51 state mapping.
-- NULL on links from before, so the first read after the upgrade says so in
-- the release history.
ALTER TABLE app_store_release_links ADD COLUMN remapped_at INTEGER;

-- Store blockers Apple or Google only name when they refuse something,
-- remembered per app until a later submission goes through, so the next
-- build's readiness check can show them before the upload.
CREATE TABLE store_readiness_memory (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    store TEXT NOT NULL CHECK (store IN ('google_play', 'app_store')),
    app_identifier TEXT NOT NULL,
    code TEXT NOT NULL,
    title TEXT NOT NULL,
    detail TEXT,
    console_url TEXT,
    recorded_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, store, app_identifier, code)
) WITHOUT ROWID;

-- Channels that filter events keep hearing about failed releases when those
-- show up under the new names: a refusal is now `release_blocked` or
-- `release_rejected` instead of a late `release_failed`, and what
-- `release_uploaded` used to cover past the upload is `release_submitted`,
-- `release_approved` and `release_live`.
UPDATE notification_channels
SET event_filter_json = (
    SELECT json_group_array(value) FROM (
        SELECT value FROM json_each(notification_channels.event_filter_json)
        UNION SELECT 'release_blocked'
        UNION SELECT 'release_rejected'
    )
)
WHERE json_valid(event_filter_json)
  AND EXISTS (
      SELECT 1 FROM json_each(notification_channels.event_filter_json)
      WHERE value = 'release_failed'
  );

UPDATE notification_channels
SET event_filter_json = (
    SELECT json_group_array(value) FROM (
        SELECT value FROM json_each(notification_channels.event_filter_json)
        UNION SELECT 'release_submitted'
        UNION SELECT 'release_approved'
        UNION SELECT 'release_live'
    )
)
WHERE json_valid(event_filter_json)
  AND EXISTS (
      SELECT 1 FROM json_each(notification_channels.event_filter_json)
      WHERE value = 'release_uploaded'
  );

COMMIT;

PRAGMA foreign_keys=ON;
