-- Named signing identities a project's pipelines and v2 files share.
-- `material_encrypted` holds the key material as encrypted JSON; `summary`
-- holds what is safe to show (file names, alias, team, profiles, expiry).
CREATE TABLE IF NOT EXISTS project_signing_identities (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('android_keystore', 'ios_distribution')),
    summary TEXT NOT NULL DEFAULT '{}',
    material_encrypted TEXT NOT NULL,
    created_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (project_id, name)
);

-- Per-workflow settings of a pipeline's v2 file. NULL columns follow the
-- file, then the pipeline.
CREATE TABLE IF NOT EXISTS pipeline_workflow_settings (
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    workflow TEXT NOT NULL,
    android_identity_id TEXT REFERENCES project_signing_identities(id) ON DELETE RESTRICT,
    ios_identity_id TEXT REFERENCES project_signing_identities(id) ON DELETE RESTRICT,
    publish_enabled INTEGER CHECK (publish_enabled IN (0, 1)),
    approval_required INTEGER CHECK (approval_required IN (0, 1)),
    updated_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (pipeline_id, workflow)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_pipeline_workflow_settings_android
    ON pipeline_workflow_settings (android_identity_id);
CREATE INDEX IF NOT EXISTS idx_pipeline_workflow_settings_ios
    ON pipeline_workflow_settings (ios_identity_id);
