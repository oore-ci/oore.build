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

INSERT INTO build_artifact_sizes (artifact_id, build_id, name, artifact_type, size_bytes, recorded_at)
SELECT id, build_id, name, artifact_type, file_size, COALESCE(finalized_at, created_at)
FROM artifacts
WHERE state = 'available' AND file_size IS NOT NULL AND file_size >= 0;
