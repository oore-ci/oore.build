-- Release environments on a project (for example dev, internal, beta and
-- production), each with its own store target, rollout policy and approval
-- rule. A build is promoted between environments instead of rebuilt.
-- Projects without environments keep the per-pipeline publish behaviour.

CREATE TABLE project_environments (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    -- Stable identifier for configuration files, such as `production`.
    env_key TEXT NOT NULL,
    name TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    store TEXT NOT NULL CHECK (store IN ('google_play', 'app_store')),
    track TEXT NOT NULL,
    approval_required INTEGER NOT NULL DEFAULT 0 CHECK (approval_required IN (0, 1)),
    policy_json TEXT NOT NULL DEFAULT '{}',
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (project_id, env_key),
    UNIQUE (project_id, store, track)
);

CREATE INDEX idx_project_environments_project ON project_environments (project_id, position);

-- The environment a release belongs to, and the release it was promoted
-- from. Both stay NULL for releases of projects without environments.
ALTER TABLE releases ADD COLUMN environment_id TEXT REFERENCES project_environments(id) ON DELETE SET NULL;
ALTER TABLE releases ADD COLUMN promoted_from_release_id TEXT REFERENCES releases(id) ON DELETE SET NULL;

CREATE INDEX idx_releases_environment ON releases (environment_id, created_at DESC);

-- A request to put a build into an environment. `pending` waits for an
-- approval and has not touched the store; `promoting` is the store call in
-- flight; `completed` points at the release it created.
CREATE TABLE environment_promotions (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    environment_id TEXT NOT NULL REFERENCES project_environments(id) ON DELETE CASCADE,
    source_release_id TEXT NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    release_id TEXT REFERENCES releases(id) ON DELETE SET NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'promoting', 'completed', 'rejected', 'failed')),
    reason TEXT,
    decision_reason TEXT,
    error TEXT,
    requested_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    decided_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    decided_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_environment_promotions_project ON environment_promotions (project_id, created_at DESC);
CREATE INDEX idx_environment_promotions_target ON environment_promotions (build_id, environment_id, state);
