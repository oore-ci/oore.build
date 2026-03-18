-- Per-project role assignments for granular RBAC (OOR-136).
-- No backfill: table starts empty. owner/admin bypass membership checks.
CREATE TABLE IF NOT EXISTS project_members (
    id          TEXT    PRIMARY KEY,
    project_id  TEXT    NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id     TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role        TEXT    NOT NULL DEFAULT 'viewer'
                        CHECK (role IN ('maintainer', 'developer', 'viewer')),
    created_by  TEXT    NOT NULL REFERENCES users(id),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE (project_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_project_members_project ON project_members(project_id);
CREATE INDEX IF NOT EXISTS idx_project_members_user    ON project_members(user_id);
