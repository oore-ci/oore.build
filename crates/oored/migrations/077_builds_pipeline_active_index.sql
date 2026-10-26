-- Counts a pipeline's active builds for its max_concurrent limit on every
-- runner claim without reading the pipeline's finished builds.
CREATE INDEX IF NOT EXISTS idx_builds_pipeline_active
    ON builds (pipeline_id, status)
    WHERE status IN ('scheduled', 'assigned', 'running');
