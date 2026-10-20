CREATE TABLE releases (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    store TEXT NOT NULL CHECK (store IN ('google_play', 'app_store')),
    track TEXT NOT NULL,
    package_name TEXT,
    version_name TEXT,
    version_code INTEGER,
    state TEXT NOT NULL CHECK (state IN (
        'uploading', 'processing', 'in_review', 'rolling',
        'paused', 'halted', 'complete', 'failed'
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
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_releases_project ON releases (project_id, created_at DESC);
CREATE INDEX idx_releases_build ON releases (build_id);
CREATE INDEX idx_releases_due ON releases (state, next_step_at);

CREATE TABLE release_history (
    id TEXT PRIMARY KEY NOT NULL,
    release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    actor_id TEXT REFERENCES users(id) ON DELETE SET NULL,
    actor_email TEXT,
    from_state TEXT,
    to_state TEXT NOT NULL,
    from_fraction REAL,
    to_fraction REAL,
    reason TEXT,
    at INTEGER NOT NULL
);

CREATE INDEX idx_release_history_release ON release_history (release_id, at);
CREATE INDEX idx_release_history_at ON release_history (at DESC);
