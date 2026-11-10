-- What a release rehearsal found, per build and store: the runner's checks
-- (Play's edit validation, altool's IPA validation) and what Oore read from
-- the store. A rehearsal build is marked in its config snapshot
-- (`"rehearsal": true`), so builds need no new column.
CREATE TABLE build_rehearsal_checks (
    build_id TEXT NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    store TEXT NOT NULL CHECK (store IN ('google_play', 'app_store')),
    position INTEGER NOT NULL,
    code TEXT NOT NULL,
    level TEXT NOT NULL CHECK (level IN ('ok', 'warn', 'block')),
    title TEXT NOT NULL,
    detail TEXT,
    console_url TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (build_id, store, position)
);
