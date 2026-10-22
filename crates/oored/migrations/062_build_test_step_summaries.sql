-- Test totals per build step. A step that reports again replaces its earlier
-- totals instead of adding to them; build_test_summaries holds the sum over
-- a build's steps.
CREATE TABLE build_test_step_summaries (
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    step TEXT NOT NULL,
    total INTEGER NOT NULL DEFAULT 0,
    passed INTEGER NOT NULL DEFAULT 0,
    failed INTEGER NOT NULL DEFAULT 0,
    skipped INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (build_id, step)
);
