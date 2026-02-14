# Local Repository Pipelines Use Manual-Only Triggers

## Status

`implemented`

## Problem

Pipeline trigger configuration exposed webhook-style event and branch filters even for local repository projects, where only manual/API rebuild flows are currently supported.

## User Impact

- Local-mode users no longer configure trigger options that cannot execute in current local repository flows.
- Pipeline creation/editing is clearer: local repository pipelines are explicitly manual-trigger oriented.
- Invalid trigger payloads for local repository projects are rejected server-side for consistency.

## UI Changes

- Pipeline form `Triggers` section now has a local manual-only state:
  - hides event and branch filter controls
  - shows a concise manual-only message
  - keeps concurrency controls (`cancel_previous`, `max_concurrent`)
- New pipeline and edit pipeline routes pass `manualOnlyTriggers` in local runtime mode.
- Pipeline detail view shows trigger mode as `manual only` in local runtime mode instead of event/branch summaries.

## API Changes

- `POST /v1/projects/{project_id}/pipelines` and `PATCH /v1/pipelines/{pipeline_id}` now enforce:
  - for projects linked to `local_git` repositories, `trigger_config.events` must be empty
  - for projects linked to `local_git` repositories, `trigger_config.branches` must be empty
- Violations return `400 invalid_trigger_config`.
- No endpoint or schema shape changes.

## Security Considerations

- Enforcement occurs on backend write paths (not only UI), preventing clients from persisting unsupported trigger policies for local repositories.
- Existing RBAC requirements for pipeline mutation remain unchanged.

## Migration and Rollout

1. Deploy backend and web updates together.
2. No schema migration required.
3. Existing local pipelines with legacy trigger filters are unaffected until edited; subsequent edits in local mode will submit manual-only trigger config.

## Acceptance Criteria

- [x] New local-mode pipelines cannot save non-empty trigger events/branches.
- [x] Editing local-mode pipelines does not expose event/branch trigger controls.
- [x] Backend rejects non-empty trigger config for local repository projects.
- [x] Pipeline detail screen reflects manual-only trigger behavior in local mode.

## Owner

Platform team

## Last Updated

`2026-02-14`
