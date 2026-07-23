-- Bind in-flight legacy builds to the exact repository they were created
-- from before any old project links are cleared. The URL comparison prevents
-- a previously relinked project from lending its current trust to an older
-- snapshot. Assigned/running builds can then drain through their build-bound
-- checkout identity even after the project is unlinked below.
UPDATE builds
SET config_snapshot = json_set(
  config_snapshot,
  '$.repository_id',
  (
    SELECT p.repository_id
    FROM projects p
    JOIN integration_repositories r ON r.id = p.repository_id
    JOIN integration_installations inst ON inst.id = r.installation_id
    JOIN integrations i ON i.id = inst.integration_id
    WHERE p.id = builds.project_id
      AND json_extract(builds.config_snapshot, '$.repo_url') = CASE
        WHEN i.provider = 'local_git' THEN r.html_url
        ELSE i.host_url || '/' || r.full_name || '.git'
      END
  )
)
WHERE status IN ('queued', 'scheduled', 'assigned', 'running')
  AND json_valid(config_snapshot)
  AND json_type(config_snapshot, '$.repository_id') IS NULL
  AND EXISTS (
    SELECT 1
    FROM projects p
    JOIN integration_repositories r ON r.id = p.repository_id
    JOIN integration_installations inst ON inst.id = r.installation_id
    JOIN integrations i ON i.id = inst.integration_id
    WHERE p.id = builds.project_id
      AND json_extract(builds.config_snapshot, '$.repo_url') = CASE
        WHEN i.provider = 'local_git' THEN r.html_url
        ELSE i.host_url || '/' || r.full_name || '.git'
      END
  );

-- A project may already have been relinked while an assigned/running build from
-- its previous source was draining. Recover that immutable source directly
-- from the snapshot URL so checkout does not fall back to the mutable project
-- link after restart. Multiple rows for the same canonical URL are equivalent
-- source identities; choose one deterministically.
UPDATE builds
SET config_snapshot = json_set(
  config_snapshot,
  '$.repository_id',
  (
    SELECT MIN(r.id)
    FROM integration_repositories r
    JOIN integration_installations inst ON inst.id = r.installation_id
    JOIN integrations i ON i.id = inst.integration_id
    WHERE json_extract(builds.config_snapshot, '$.repo_url') = CASE
      WHEN i.provider = 'local_git' THEN r.html_url
      ELSE i.host_url || '/' || r.full_name || '.git'
    END
  )
)
WHERE status IN ('queued', 'scheduled', 'assigned', 'running')
  AND json_valid(config_snapshot)
  AND json_type(config_snapshot, '$.repository_id') IS NULL
  AND EXISTS (
    SELECT 1
    FROM integration_repositories r
    JOIN integration_installations inst ON inst.id = r.installation_id
    JOIN integrations i ON i.id = inst.integration_id
    WHERE json_extract(builds.config_snapshot, '$.repo_url') = CASE
      WHEN i.provider = 'local_git' THEN r.html_url
      ELSE i.host_url || '/' || r.full_name || '.git'
    END
  );

-- A queued or merely scheduled snapshot from an older project source cannot be
-- retrusted by a later link. Cancel it rather than leaving permanent blocked
-- work behind after the source-identity migration.
INSERT INTO build_events (
  id,
  build_id,
  from_status,
  to_status,
  actor,
  reason,
  created_at
)
SELECT
  lower(hex(randomblob(16))),
  b.id,
  b.status,
  'canceled',
  'system',
  'Canceled during source-identity migration because the project source changed; trigger a new build',
  unixepoch()
FROM builds b
JOIN projects p ON p.id = b.project_id
WHERE b.status IN ('queued', 'scheduled')
  AND json_extract(b.config_snapshot, '$.repository_id') IS NOT p.repository_id;

UPDATE builds
SET status = 'canceled',
    runner_id = NULL,
    signing_token_hash = NULL,
    finished_at = unixepoch(),
    updated_at = unixepoch()
WHERE status IN ('queued', 'scheduled')
  AND EXISTS (
    SELECT 1
    FROM projects p
    WHERE p.id = builds.project_id
      AND json_extract(builds.config_snapshot, '$.repository_id') IS NOT p.repository_id
  );

-- Unassigned builds held only by the retired repository approval must not begin executing
-- merely because the approval column is removed. An Owner/Admin can choose the
-- project source again and trigger a fresh build if it is still wanted.
INSERT INTO build_events (
  id,
  build_id,
  from_status,
  to_status,
  actor,
  reason,
  created_at
)
SELECT
  lower(hex(randomblob(16))),
  b.id,
  b.status,
  'canceled',
  'system',
  'Canceled while simplifying Direct runner trust; an Owner/Admin must choose the project source again before rerunning',
  unixepoch()
FROM builds b
JOIN projects p ON p.id = b.project_id
JOIN integration_repositories r ON r.id = p.repository_id
WHERE b.status IN ('queued', 'scheduled')
  AND r.allow_direct_macos_runner = 0;

UPDATE builds
SET status = 'canceled',
    runner_id = NULL,
    signing_token_hash = NULL,
    finished_at = unixepoch(),
    updated_at = unixepoch()
WHERE status IN ('queued', 'scheduled')
  AND project_id IN (
    SELECT p.id
    FROM projects p
    JOIN integration_repositories r ON r.id = p.repository_id
    WHERE r.allow_direct_macos_runner = 0
  );

-- A legacy project link was not itself an execution grant: Developers could
-- create one while the repository stayed blocked. Require an Owner/Admin to
-- make the new project-level trust decision instead of silently promoting the
-- old link when the repository gate disappears.
UPDATE projects
SET repository_id = NULL,
    updated_at = unixepoch()
WHERE repository_id IN (
  SELECT id
  FROM integration_repositories
  WHERE allow_direct_macos_runner = 0
);

ALTER TABLE integration_repositories
  DROP COLUMN allow_direct_macos_runner;

-- Turn the retired opt-in gate into a default-on operational pause. A stored 0
-- was usually just the old default, not an operator decision, so it must not
-- keep upgraded installations inert. Preserve an explicit operator stop only
-- when the policy audit trail proves the old switch was deliberately turned
-- off after having been enabled.
ALTER TABLE instance_preferences
  RENAME COLUMN direct_macos_runner_enabled TO direct_macos_runner_paused;

UPDATE instance_preferences
SET direct_macos_runner_paused = CASE
  WHEN direct_macos_runner_paused = 0
    AND EXISTS (
      SELECT 1
      FROM audit_logs
      WHERE action = 'direct_macos_runner_policy_updated'
        AND resource_type = 'instance_settings'
        AND resource_id = 'direct_macos_runner'
        AND json_valid(details)
        AND json_extract(details, '$.direct_macos_runner_enabled') = 0
    )
    THEN 1
  ELSE 0
END;
