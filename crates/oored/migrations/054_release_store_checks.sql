-- When the release monitor last compared a Google Play release with the track
-- in Play Console, so a release discarded or changed there is noticed.
ALTER TABLE releases ADD COLUMN store_checked_at INTEGER;
