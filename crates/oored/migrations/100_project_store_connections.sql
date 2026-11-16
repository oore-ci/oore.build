-- Store connections per app: Google Play and App Store Connect are connected
-- once for an app, and its pipelines only pick a track.
--
-- Credentials stay where they are stored today. A Google Play connection
-- names a secret (resolved app -> instance, as before); an App Store Connect
-- connection points at a stored App Store Connect key.

CREATE TABLE IF NOT EXISTS project_store_connections (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    store TEXT NOT NULL CHECK (store IN ('google_play', 'app_store')),
    -- google_play: the secret holding the service account JSON key.
    service_account_secret TEXT,
    -- app_store: the stored App Store Connect API key.
    app_store_connect_key_id TEXT,
    -- The package name (Google Play) or bundle ID (App Store Connect).
    app_identifier TEXT,
    -- The pipeline the migration took this connection from, when it did.
    migrated_from_pipeline_id TEXT,
    -- The latest connection test, as JSON, and what it was run against
    -- (the secret version or the key's updated_at).
    last_check TEXT,
    last_check_fingerprint TEXT,
    created_by TEXT,
    updated_by TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (app_store_connect_key_id) REFERENCES app_store_connect_keys(id) ON DELETE RESTRICT,
    FOREIGN KEY (created_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY (updated_by) REFERENCES users(id) ON DELETE SET NULL,
    UNIQUE (project_id, store),
    CHECK (
        (store = 'google_play' AND service_account_secret IS NOT NULL)
        OR (store = 'app_store' AND app_store_connect_key_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_project_store_connections_key
ON project_store_connections(app_store_connect_key_id);

-- ── Google Play ─────────────────────────────────────────────────
-- Each app takes the service account secret of one pipeline: an enabled
-- one first, then the most recently updated, then the lowest pipeline id.

INSERT INTO project_store_connections (
    id, project_id, store, service_account_secret, app_identifier,
    migrated_from_pipeline_id, created_by, updated_by, created_at, updated_at
)
SELECT lower(hex(randomblob(16))), ranked.project_id, 'google_play', ranked.secret,
       ranked.package_name, ranked.pipeline_id, ranked.updated_by, ranked.updated_by,
       ranked.updated_at, ranked.updated_at
FROM (
    SELECT p.project_id, t.pipeline_id, t.updated_by, t.updated_at,
           trim(json_extract(t.config, '$.service_account_secret')) AS secret,
           NULLIF(trim(json_extract(t.config, '$.package_name')), '') AS package_name,
           ROW_NUMBER() OVER (
               PARTITION BY p.project_id
               ORDER BY t.enabled DESC, t.updated_at DESC, t.pipeline_id ASC
           ) AS position
    FROM pipeline_publish_targets t
    JOIN pipelines p ON p.id = t.pipeline_id
    WHERE t.store = 'google_play'
      AND json_valid(t.config)
      AND trim(COALESCE(json_extract(t.config, '$.service_account_secret'), '')) != ''
) AS ranked
WHERE ranked.position = 1;

-- Pipelines that named the app's secret now follow the app (an empty
-- secret means "the app's connection"). A pipeline that named another
-- secret keeps it as its own override.
UPDATE pipeline_publish_targets
SET config = json_set(config, '$.service_account_secret', '')
WHERE store = 'google_play'
  AND json_valid(config)
  AND trim(COALESCE(json_extract(config, '$.service_account_secret'), '')) = (
      SELECT c.service_account_secret FROM project_store_connections c
      JOIN pipelines p ON p.project_id = c.project_id
      WHERE p.id = pipeline_publish_targets.pipeline_id AND c.store = 'google_play'
  );

-- The same for the expected package: one that matches the app's follows it.
UPDATE pipeline_publish_targets
SET config = json_remove(config, '$.package_name')
WHERE store = 'google_play'
  AND json_valid(config)
  AND trim(COALESCE(json_extract(config, '$.package_name'), '')) = (
      SELECT c.app_identifier FROM project_store_connections c
      JOIN pipelines p ON p.project_id = c.project_id
      WHERE p.id = pipeline_publish_targets.pipeline_id AND c.store = 'google_play'
  );

-- ── App Store Connect ───────────────────────────────────────────
-- A pipeline's own API key is copied into the stored keys once per distinct
-- key (key ID and issuer), unless a stored key already holds it. The
-- encrypted private key is copied as it is: nobody re-enters it.

CREATE TEMP TABLE migrate_own_asc_keys AS
SELECT lower(hex(randomblob(16))) AS id, own.api_key_id, own.api_issuer_id,
       own.api_private_key_encrypted, own.project_name, own.updated_at
FROM (
    SELECT s.api_key_id, s.api_issuer_id, s.api_private_key_encrypted,
           pr.name AS project_name, s.updated_at,
           ROW_NUMBER() OVER (
               PARTITION BY s.api_key_id, s.api_issuer_id
               ORDER BY s.updated_at DESC, s.pipeline_id ASC
           ) AS position
    FROM pipeline_ios_signing_settings s
    JOIN pipelines p ON p.id = s.pipeline_id
    JOIN projects pr ON pr.id = p.project_id
    WHERE s.shared_api_key_id IS NULL
      AND s.api_key_id IS NOT NULL
      AND s.api_issuer_id IS NOT NULL
      AND s.api_private_key_encrypted IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM app_store_connect_keys k
          WHERE k.key_id = s.api_key_id AND k.issuer_id = s.api_issuer_id
      )
) AS own
WHERE own.position = 1;

-- Named after the app and the key ID, made unique when that name is taken.
INSERT INTO app_store_connect_keys (
    id, name, key_id, issuer_id, private_key_encrypted, created_at, updated_at
)
SELECT m.id,
       CASE
           WHEN NOT EXISTS (
               SELECT 1 FROM app_store_connect_keys k
               WHERE k.name = substr(m.project_name, 1, 40) || ' (' || m.api_key_id || ')'
           ) AND NOT EXISTS (
               SELECT 1 FROM migrate_own_asc_keys other
               WHERE other.id < m.id
                 AND substr(other.project_name, 1, 40) || ' (' || other.api_key_id || ')'
                   = substr(m.project_name, 1, 40) || ' (' || m.api_key_id || ')'
           )
           THEN substr(m.project_name, 1, 40) || ' (' || m.api_key_id || ')'
           ELSE substr(m.project_name, 1, 30) || ' (' || m.api_key_id || ') ' || substr(m.id, 1, 6)
       END,
       m.api_key_id, m.api_issuer_id, m.api_private_key_encrypted, m.updated_at, m.updated_at
FROM migrate_own_asc_keys m;

DROP TABLE migrate_own_asc_keys;

-- Each app takes the key of one pipeline: one that uploads to App Store
-- Connect first, then the most recently updated signing settings, then the
-- lowest pipeline id.
INSERT INTO project_store_connections (
    id, project_id, store, app_store_connect_key_id, app_identifier,
    migrated_from_pipeline_id, created_by, updated_by, created_at, updated_at
)
SELECT lower(hex(randomblob(16))), ranked.project_id, 'app_store', ranked.key_ref,
       (SELECT l.bundle_id FROM app_store_release_links l
        WHERE l.pipeline_id = ranked.pipeline_id
        ORDER BY l.created_at DESC LIMIT 1),
       ranked.pipeline_id, ranked.updated_by, ranked.updated_by,
       ranked.updated_at, ranked.updated_at
FROM (
    SELECT p.project_id, s.pipeline_id, s.updated_by, s.updated_at,
           COALESCE(
               s.shared_api_key_id,
               (SELECT k.id FROM app_store_connect_keys k
                WHERE k.key_id = s.api_key_id AND k.issuer_id = s.api_issuer_id
                ORDER BY k.created_at ASC, k.id ASC LIMIT 1)
           ) AS key_ref,
           ROW_NUMBER() OVER (
               PARTITION BY p.project_id
               ORDER BY EXISTS (
                            SELECT 1 FROM pipeline_publish_targets t
                            WHERE t.pipeline_id = s.pipeline_id
                              AND t.store = 'app_store' AND t.enabled = 1
                        ) DESC,
                        s.updated_at DESC, s.pipeline_id ASC
           ) AS position
    FROM pipeline_ios_signing_settings s
    JOIN pipelines p ON p.id = s.pipeline_id
    WHERE s.shared_api_key_id IS NOT NULL
       OR (s.api_key_id IS NOT NULL AND s.api_issuer_id IS NOT NULL
           AND s.api_private_key_encrypted IS NOT NULL)
) AS ranked
WHERE ranked.position = 1 AND ranked.key_ref IS NOT NULL;

-- A pipeline whose key is a different one keeps uploading with it: its App
-- Store publish settings get `upload_with_signing_key`. One without publish
-- settings gets them, off, so release monitoring keeps its key too.
INSERT INTO pipeline_publish_targets (
    id, pipeline_id, store, enabled, config, created_at, updated_at
)
SELECT lower(hex(randomblob(16))), s.pipeline_id, 'app_store', 0,
       '{"enabled":false,"destination":"testflight","testflight_groups":[],"release_notes":[]}',
       s.updated_at, s.updated_at
FROM pipeline_ios_signing_settings s
JOIN pipelines p ON p.id = s.pipeline_id
JOIN project_store_connections c ON c.project_id = p.project_id AND c.store = 'app_store'
JOIN app_store_connect_keys app_key ON app_key.id = c.app_store_connect_key_id
LEFT JOIN app_store_connect_keys shared ON shared.id = s.shared_api_key_id
WHERE (shared.id IS NOT NULL OR s.api_private_key_encrypted IS NOT NULL)
  AND (COALESCE(shared.key_id, s.api_key_id) != app_key.key_id
       OR COALESCE(shared.issuer_id, s.api_issuer_id) != app_key.issuer_id)
  AND NOT EXISTS (
      SELECT 1 FROM pipeline_publish_targets t
      WHERE t.pipeline_id = s.pipeline_id AND t.store = 'app_store'
  );

UPDATE pipeline_publish_targets
SET config = json_set(config, '$.upload_with_signing_key', json('true'))
WHERE store = 'app_store'
  AND json_valid(config)
  AND pipeline_id IN (
      SELECT s.pipeline_id
      FROM pipeline_ios_signing_settings s
      JOIN pipelines p ON p.id = s.pipeline_id
      JOIN project_store_connections c ON c.project_id = p.project_id AND c.store = 'app_store'
      JOIN app_store_connect_keys app_key ON app_key.id = c.app_store_connect_key_id
      LEFT JOIN app_store_connect_keys shared ON shared.id = s.shared_api_key_id
      WHERE (shared.id IS NOT NULL OR s.api_private_key_encrypted IS NOT NULL)
        AND (COALESCE(shared.key_id, s.api_key_id) != app_key.key_id
             OR COALESCE(shared.issuer_id, s.api_issuer_id) != app_key.issuer_id)
  );
