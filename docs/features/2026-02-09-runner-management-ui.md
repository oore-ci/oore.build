# Runner Management UI

## Status

`ready`

## Problem

Runner inventory existed only through API/CLI responses. Operators had no first-class UI surface to see runner status and heartbeats or to rename externally registered runners. This made day-to-day operations slower and error-prone, especially when multiple runners were present.

## User Impact

- **Owners/Admins** can open a dedicated Runners settings page to view all registered runners, status, heartbeat freshness, and capabilities.
- **Owners/Admins** can rename externally registered runners directly in the UI.
- **Embedded runner safety** is enforced: embedded local runners remain visible but cannot be renamed.
- **Developers/QA viewers** do not get rename capabilities and cannot access the admin settings page.

## UI Changes

- Added `Settings -> Runners` route in `apps/web`.
- Added runner table columns: name, status, last heartbeat, capabilities, registered by, updated time, actions.
- Added rename dialog using shadcn Form (`react-hook-form` + `zod`) for external runners.
- Rename button is disabled for embedded runners (`registered_by == null`) with helper text.
- Added success/error toast feedback on rename operations.

## API Changes

- Added `PATCH /v1/runners/{runner_id}`.
- New request contract:
  - `UpdateRunnerRequest { name?: string }`
- New response contract:
  - `UpdateRunnerResponse { runner: Runner }`
- Endpoint behavior:
  - Requires `runners:write`.
  - Validates trimmed name length (1..255).
  - Returns `404 not_found` when runner does not exist.
  - Returns `409 embedded_runner_locked` when `registered_by IS NULL`.
  - Writes audit log action `runner_renamed` on successful name change.

## Security Considerations

- Rename remains permission-gated by existing RBAC (`runners:write`).
- Embedded runner rename is blocked to prevent daemon-managed identity drift.
- Rename actions are auditable (`audit_logs.action = runner_renamed`) with actor and resource IDs.
- Runner token model and runner-scoped auth are unchanged.

## Migration and Rollout

- No schema migration required.
- API is additive and backward-compatible.
- UI route is additive under existing admin settings navigation.
- Existing CLI registration/start flows remain unchanged.

## Acceptance Criteria

- [x] Owner/Admin can view runners in UI.
- [x] Owner/Admin can rename non-embedded runners.
- [x] Embedded runners cannot be renamed (UI disabled + API conflict).
- [x] Runner rename writes an audit log entry.
- [x] Docs and docs-site include runner management guidance.
- [x] `make validate` passes.

## Owner

oore.build team

## Last Updated

`2026-02-09`
