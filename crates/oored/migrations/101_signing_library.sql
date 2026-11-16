-- The app's signing library becomes the only home for keystores,
-- certificates and provisioning profiles. A pipeline now picks a library
-- identity per platform (and per Android build type) instead of holding its
-- own files. The pipeline's own columns stay stored: the daemon copies them
-- into library identities on start (it needs the instance key to do so) and
-- links them here, so nothing is re-entered and builds keep the same key.

-- A keyed hash of the identity's key material, so the same keystore or
-- certificate uploaded on several pipelines becomes one identity.
ALTER TABLE project_signing_identities ADD COLUMN content_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_project_signing_identities_hash
    ON project_signing_identities (project_id, content_hash);

-- When set on an enabled profile, the build type signs with this identity.
ALTER TABLE pipeline_android_signing_profiles
ADD COLUMN identity_id TEXT REFERENCES project_signing_identities(id) ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_pipeline_android_signing_identity
    ON pipeline_android_signing_profiles (identity_id);

-- When set and the mode is manual, iOS builds sign with this identity; the
-- team, export method and bundle IDs are copied from it.
ALTER TABLE pipeline_ios_signing_settings
ADD COLUMN identity_id TEXT REFERENCES project_signing_identities(id) ON DELETE RESTRICT;

CREATE INDEX IF NOT EXISTS idx_pipeline_ios_signing_identity
    ON pipeline_ios_signing_settings (identity_id);
