-- Config files Oore found in a project's repository, per branch, as of the
-- branch head it last read. Pipeline health and "Found in repository" read
-- this cache instead of calling the source provider.
CREATE TABLE repository_config_scans (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    branch TEXT NOT NULL,
    -- The commit the files were read at; NULL until a read succeeds.
    sha TEXT,
    truncated INTEGER NOT NULL DEFAULT 0,
    -- Why the last read failed. The files from the read before it remain.
    error TEXT,
    scanned_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, branch)
) WITHOUT ROWID;

CREATE TABLE repository_config_files (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    branch TEXT NOT NULL,
    path TEXT NOT NULL,
    sha TEXT NOT NULL,
    valid INTEGER NOT NULL,
    workflows TEXT NOT NULL DEFAULT '[]',
    errors TEXT NOT NULL DEFAULT '[]',
    -- When the file first appeared on this branch.
    seen_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, branch, path)
) WITHOUT ROWID;

-- Add a pipeline for each new config file on the default branch. Off for
-- existing projects; new projects turn it on when they are created.
ALTER TABLE projects ADD COLUMN auto_add_pipelines INTEGER NOT NULL DEFAULT 0;

-- config_path always names the file. Pipelines that tried `.oore.yaml` then
-- `.oore.yml` now name `.oore.yaml`, which the runner still reads as
-- `.oore.yml` when only that exists. config_path_explicit stays 0 on them
-- until the first read of the default branch writes the file it found.
UPDATE pipelines SET config_path = '.oore.yaml' WHERE config_path_explicit = 0;
