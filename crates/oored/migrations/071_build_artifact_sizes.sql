-- Artifact sizes per build, kept after retention deletes the artifact rows so
-- later builds can still compare against them. Keyed by artifact id because
-- one build may publish several files with the same name.
CREATE TABLE build_artifact_sizes (
    artifact_id TEXT PRIMARY KEY NOT NULL,
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    artifact_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    recorded_at INTEGER NOT NULL
);

CREATE INDEX idx_build_artifact_sizes_build ON build_artifact_sizes (build_id);

-- The baseline lookup walks one pipeline's builds on one branch, newest first.
CREATE INDEX idx_builds_pipeline_branch_number ON builds (pipeline_id, branch, build_number);

INSERT INTO build_artifact_sizes (artifact_id, build_id, name, artifact_type, size_bytes, recorded_at)
SELECT a.id, a.build_id, a.name, a.artifact_type, a.file_size,
       COALESCE(a.finalized_at, a.created_at)
FROM artifacts a
JOIN builds b ON b.id = a.build_id
WHERE a.state = 'available' AND a.file_size IS NOT NULL AND a.file_size >= 0;
