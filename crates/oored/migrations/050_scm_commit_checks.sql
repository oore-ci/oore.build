-- Report build progress to GitHub check runs and GitLab commit statuses.

-- The build status last reported to the source; a build is due for a report
-- while this differs from `status`. Existing builds start reported so the
-- first run does not post statuses for history.
ALTER TABLE builds ADD COLUMN scm_reported_status TEXT;
ALTER TABLE builds ADD COLUMN scm_report_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE builds ADD COLUMN scm_report_after INTEGER;
UPDATE builds SET scm_reported_status = status;

CREATE INDEX IF NOT EXISTS idx_builds_scm_report_due
ON builds(created_at) WHERE scm_reported_status IS NOT status;

-- One GitHub check run per pipeline, commit and platform, so a rerun or a
-- second build of the same commit updates the check instead of adding one.
CREATE TABLE IF NOT EXISTS scm_check_runs (
    pipeline_id TEXT NOT NULL REFERENCES pipelines(id) ON DELETE CASCADE,
    commit_sha TEXT NOT NULL,
    platform TEXT NOT NULL,
    external_id TEXT NOT NULL,
    build_id TEXT REFERENCES builds(id) ON DELETE SET NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (pipeline_id, commit_sha, platform)
);
