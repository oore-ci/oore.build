-- What a build is, in the words people use: the commit's title and author,
-- and the app version it builds. The runner reports them after checkout, so
-- they stay NULL for queued builds and for builds from older runners.
ALTER TABLE builds ADD COLUMN commit_title TEXT;
ALTER TABLE builds ADD COLUMN commit_author TEXT;
ALTER TABLE builds ADD COLUMN version_name TEXT;
ALTER TABLE builds ADD COLUMN version_code INTEGER;

-- The newer build that canceled this one when it was pushed to the same
-- branch, so the build can say "Replaced by #147".
ALTER TABLE builds ADD COLUMN superseded_by_build_id TEXT;
ALTER TABLE builds ADD COLUMN superseded_by_build_number INTEGER;
