-- Why the last crash-rate check of a release failed. While set on a release
-- with a crash-rate threshold, the rollout monitor holds scheduled steps.
ALTER TABLE releases ADD COLUMN health_error TEXT;

-- When App Store Connect polls for a release started failing without a
-- success since, so the monitor can give up on a release that never recovers.
ALTER TABLE app_store_release_links ADD COLUMN failing_since INTEGER;
