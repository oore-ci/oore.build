-- The runner that ran a build. `runner_id` is cleared when a build finishes,
-- revoking the runner's access to it, so finished builds keep the runner here
-- for display.
ALTER TABLE builds ADD COLUMN ran_on_runner_id TEXT;

-- Earlier builds: the last runner that moved the build.
UPDATE builds
SET ran_on_runner_id = (
    SELECT substr(build_events.actor, 8)
    FROM build_events
    WHERE build_events.build_id = builds.id
      AND build_events.actor LIKE 'runner:%'
    ORDER BY build_events.created_at DESC
    LIMIT 1
)
WHERE ran_on_runner_id IS NULL;
