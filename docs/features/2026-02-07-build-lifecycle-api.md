# Build Lifecycle API

## Status

`released`

## Problem

The platform needs a foundational build domain to track CI builds from creation through completion. Without a structured state machine, build status transitions are inconsistent, concurrent modifications cause race conditions, and there is no audit trail for debugging failed builds.

## User Impact

- **Developers** can view build history, see build status in real time, and cancel queued or running builds.
- **Admins/Owners** can audit all build state transitions with actor and reason tracking.
- **Webhook-triggered builds** create immutable records with commit SHA, config snapshot, and trigger metadata for reproducibility.

## UI Changes

Build management UI:

- `/builds` — paginated build list with status badges, trigger type, branch, commit SHA, and actor. Filterable by project, pipeline, status, and branch.
- `/builds/$buildId` — build detail page with metadata (branch, commit, actor, timestamps) and events timeline showing every state transition with actor and reason.
- Cancel button on build detail page for non-terminal builds.
- Navigation: "Builds" item in Platform sidebar group (visible to all roles with builds:read).
- Manual trigger CTA is now exposed in the web UI:
- Project detail header and empty-state CTA
- Pipeline detail header and empty-state CTA
- Project pipelines table row-level `Run` action
- Builds index header and empty-state CTA
- Manual trigger dialog supports project/pipeline selection (global), branch or commit pinning, and deep-links to created build.

## API Changes

Implemented endpoints:

- `POST /v1/projects/{project_id}/builds` — create a build (manual/API trigger). RBAC: `builds:write`. Creates immutable build record with config snapshot. Applies concurrency policy if configured.
- `GET /v1/builds` — list builds with filters (project_id, pipeline_id, status, branch, limit, offset). RBAC: `builds:read`.
- `GET /v1/builds/{build_id}` — build detail with events timeline. RBAC: `builds:read`.
- `POST /v1/builds/{build_id}/cancel` — cancel a build in any non-terminal state. RBAC: `builds:cancel`.

Build status state machine (9 states):

```
queued → scheduled → assigned → running → succeeded
  ↓          ↓           ↓          ↓
canceled   canceled   canceled   canceled
  or         or       timed_out  timed_out
expired    expired               or failed
```

Schema (migration 005):

- `projects` — linked to integration_repositories, with settings JSON
- `pipelines` — per-project, with config_path, trigger_config, concurrency policy
- `builds` — immutable build records with 9-state CHECK constraint, per-project sequential build_number, trigger metadata, config_snapshot JSON
- `build_events` — state transition audit log (from_status, to_status, actor, reason)
- `runners` — registered build agents with token_hash, capabilities, heartbeat
- `artifacts` — build output metadata (file_path, checksum, type)

Config snapshot (immutable, captured at build creation):

```json
{
  "snapshot_version": 1,
  "config_path": ".oore.yml",
  "trigger_type": "manual",
  "commit_sha": "abc123",
  "branch": "main",
  "captured_at": 1707300000
}
```

## Security Considerations

- Build state transitions use optimistic locking (`WHERE status = ?` in UPDATE) to prevent concurrent modification races. Returns 409 Conflict on conflict.
- All state transitions produce an audit event in `build_events` with actor and reason.
- Config snapshot is immutable after creation — cannot be modified post-build.
- RBAC enforced on all build endpoints (read, write, cancel per role).
- Build numbers are per-project sequential, preventing confusion across projects.

## Migration and Rollout

- Migration 005 adds 6 new tables with foreign keys to SCM integration tables.
- No changes to existing setup/auth/user flows.
- Build domain is additive — projects and pipelines must be created before builds can be triggered.
- Existing RBAC policy already includes builds, projects, pipelines, artifacts, runners resources.

## Acceptance Criteria

- [x] Build state machine with 9 states and validated transitions.
- [x] Optimistic locking prevents concurrent state modification.
- [x] Every state transition creates an audit event.
- [x] Config snapshot captured at creation time is immutable.
- [x] Manual build creation via API with RBAC.
- [x] Build list with filtering and pagination.
- [x] Build detail with events timeline.
- [x] Build cancellation from any non-terminal state.
- [x] Per-project sequential build numbers.
- [x] Webhook-triggered builds create records with trigger metadata.
- [x] Manual build trigger is available from project, pipeline, and builds UI surfaces.

## Owner

Platform Team

## Last Updated

`2026-02-09`
