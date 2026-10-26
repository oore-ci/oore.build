-- The `dart analyze` summary a runner reports for a build: counts by
-- severity and the most severe diagnostics as JSON. Later builds compare
-- their counts with a baseline build's.
CREATE TABLE build_analyses (
    build_id TEXT PRIMARY KEY NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    errors INTEGER NOT NULL,
    warnings INTEGER NOT NULL,
    infos INTEGER NOT NULL,
    diagnostics TEXT NOT NULL DEFAULT '[]',
    recorded_at INTEGER NOT NULL
);
