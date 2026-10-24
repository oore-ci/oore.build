-- App Store environments on the `app_store` track can submit the version for
-- App Review once a promotion attaches the build.
ALTER TABLE project_environments ADD COLUMN submit_for_review INTEGER NOT NULL DEFAULT 0
    CHECK (submit_for_review IN (0, 1));
