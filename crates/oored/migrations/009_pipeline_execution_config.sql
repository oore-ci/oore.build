-- Add pipeline execution fallback config and explicit config-path policy.
-- Existing pipelines are backfilled with Android fallback defaults.

ALTER TABLE pipelines
    ADD COLUMN config_path_explicit INTEGER NOT NULL DEFAULT 0;

ALTER TABLE pipelines
    ADD COLUMN execution_config TEXT NOT NULL DEFAULT '{}';

UPDATE pipelines
SET execution_config = '{"platforms":["android"],"commands":{"pre_build":[],"build":[],"post_build":[]},"artifact_patterns":["*.apk"]}'
WHERE execution_config IS NULL
   OR execution_config = ''
   OR execution_config = '{}';
