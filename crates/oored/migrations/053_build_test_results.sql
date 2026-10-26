-- Test results reported by runners. Only failures are stored per test; a
-- build with a summary row and no failure row for a test counts as a pass in
-- the flaky history.
CREATE TABLE build_test_summaries (
    build_id TEXT PRIMARY KEY NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    pipeline_id TEXT NOT NULL,
    total INTEGER NOT NULL DEFAULT 0,
    passed INTEGER NOT NULL DEFAULT 0,
    failed INTEGER NOT NULL DEFAULT 0,
    skipped INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_build_test_summaries_pipeline ON build_test_summaries (pipeline_id, created_at DESC);

CREATE TABLE build_test_failures (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    test_key TEXT NOT NULL,
    suite TEXT NOT NULL,
    name TEXT NOT NULL,
    step TEXT NOT NULL,
    message TEXT,
    stack_trace TEXT,
    duration_ms INTEGER,
    UNIQUE (build_id, test_key)
);

CREATE INDEX idx_build_test_failures_key ON build_test_failures (test_key, build_id);
