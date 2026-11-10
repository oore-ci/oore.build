-- A release freeze holds store uploads, release changes that move users to
-- a new version, and promotions, for one app or the whole instance.
CREATE TABLE release_freezes (
    id TEXT PRIMARY KEY NOT NULL,
    -- NULL freezes every app.
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    -- project_id, or '*' for the instance: one freeze per scope.
    scope TEXT NOT NULL UNIQUE,
    reason TEXT NOT NULL,
    created_by TEXT,
    created_at INTEGER NOT NULL
);
