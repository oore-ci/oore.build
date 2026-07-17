ALTER TABLE integration_repositories
  ADD COLUMN allow_direct_macos_runner INTEGER NOT NULL DEFAULT 0
  CHECK (allow_direct_macos_runner IN (0, 1));

ALTER TABLE instance_preferences
  ADD COLUMN direct_macos_runner_enabled INTEGER NOT NULL DEFAULT 0
  CHECK (direct_macos_runner_enabled IN (0, 1));
