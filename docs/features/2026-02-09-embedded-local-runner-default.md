# Embedded Local Runner Default

## Status

`ready`

## Problem

Single-host V1 setups could leave builds permanently in `queued` state unless an operator manually started a separate runner process (`oore runner start`). This added avoidable setup friction and made first-run behavior confusing.

## User Impact

- New single-host macOS installs can execute queued builds immediately after daemon startup.
- Operators no longer need to manually register/start a runner just to run the first pipeline on the same host.
- Advanced multi-runner workflows remain available through explicit runner registration/start.

## UI Changes

No UI layout changes.

Behavioral impact:
- Build timelines now progress out of `queued` on single-host installs without requiring manual runner startup.

## API Changes

No API surface changes.

Existing runner APIs remain unchanged:
- `POST /v1/runners/register`
- `POST /v1/runners/{runner_id}/heartbeat`
- `POST /v1/runners/{runner_id}/claim`
- `POST /v1/runners/{runner_id}/jobs/{job_id}/status`

New runtime behavior in daemon:
- `oored` now starts an embedded local runner by default (`OORED_RUNNER_MODE=embedded`).
- Runtime mode can be configured with `OORED_RUNNER_MODE` values: `embedded`, `external`, `hybrid`.

## Security Considerations

- Embedded runner token is generated at daemon startup and stored hashed in `runners.token_hash` (same as externally registered runners).
- Embedded runner registration does not expose additional public endpoints.
- Runner auth and per-runner job mutation checks remain unchanged.

## Migration and Rollout

- Backward compatible: external runner registration/start still works.
- Default rollout is enabled for `oored run`.
- Operators can disable embedded runner with `OORED_RUNNER_MODE=external`.

## Acceptance Criteria

- [x] Starting `oored` on a single host allows queued builds to be claimed without manual `oore runner start`.
- [x] External runner registration/start continues to function.
- [x] Embedded runner mode is configurable via environment variable.

## Owner

Platform Runtime Team

## Last Updated

`2026-02-09`
