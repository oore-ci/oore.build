-- Notes testers send about a build from the QA portal. A screenshot, when
-- attached, lives in artifact storage under screenshot_key.
CREATE TABLE build_feedback (
    id TEXT PRIMARY KEY,
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    author_user_id TEXT NOT NULL,
    author_email TEXT NOT NULL,
    message TEXT NOT NULL,
    device TEXT,
    os_version TEXT,
    app_version TEXT,
    artifact_id TEXT,
    screenshot_key TEXT,
    screenshot_content_type TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_build_feedback_build ON build_feedback(build_id, created_at);
CREATE INDEX idx_build_feedback_author ON build_feedback(author_user_id, created_at);
