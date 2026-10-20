-- Write-only secrets scoped to the instance, a project or a pipeline.
-- Values are encrypted at rest with the instance encryption key.

CREATE TABLE IF NOT EXISTS secrets (
    id TEXT PRIMARY KEY,
    scope TEXT NOT NULL CHECK (scope IN ('instance', 'project', 'pipeline')),
    project_id TEXT,
    pipeline_id TEXT,
    name TEXT NOT NULL,
    value_encrypted TEXT NOT NULL,
    created_by TEXT,
    updated_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (pipeline_id) REFERENCES pipelines(id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (updated_by) REFERENCES users(id) ON DELETE SET NULL,
    CHECK (
        (scope = 'instance' AND project_id IS NULL AND pipeline_id IS NULL)
        OR (scope = 'project' AND project_id IS NOT NULL AND pipeline_id IS NULL)
        OR (scope = 'pipeline' AND project_id IS NOT NULL AND pipeline_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_secrets_scope_name
ON secrets(scope, COALESCE(project_id, ''), COALESCE(pipeline_id, ''), name);

CREATE INDEX IF NOT EXISTS idx_secrets_project ON secrets(project_id);
CREATE INDEX IF NOT EXISTS idx_secrets_pipeline ON secrets(pipeline_id);
