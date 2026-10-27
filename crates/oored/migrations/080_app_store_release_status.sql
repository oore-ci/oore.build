-- App Store Connect's own status for a release's version or build, such as
-- WAITING_FOR_REVIEW, so the UI can say what Apple says.
ALTER TABLE app_store_release_links ADD COLUMN store_status TEXT;
