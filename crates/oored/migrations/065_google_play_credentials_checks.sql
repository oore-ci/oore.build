-- The latest Google Play credentials check for each pipeline, so the
-- Publishing tab keeps showing it after a reload. `secret_version` names the
-- secret row and change the check ran against; a rotated, deleted or
-- overridden key no longer matches, and the result is not shown.

CREATE TABLE IF NOT EXISTS pipeline_google_play_checks (
    pipeline_id TEXT PRIMARY KEY,
    service_account_secret TEXT NOT NULL,
    secret_version TEXT NOT NULL,
    ok INTEGER NOT NULL CHECK (ok IN (0, 1)),
    message TEXT NOT NULL,
    package_name TEXT,
    service_account_email TEXT,
    checked_at INTEGER NOT NULL,
    FOREIGN KEY (pipeline_id) REFERENCES pipelines(id) ON DELETE CASCADE
);
