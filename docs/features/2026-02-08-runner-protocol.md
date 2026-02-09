# Runner Protocol

## Status

`ready`

## Problem

oore.build needs a mechanism for build execution hosts (runners) to register with the daemon, advertise capabilities, and receive work. Without a runner protocol, builds remain queued indefinitely with no path to execution. The protocol must define how runners authenticate, how the daemon tracks runner availability, and how work is distributed — all over the HTTPS JSON transport specified in the platform contract (section 16).

## User Impact

- **Operators** register macOS hosts as runners via `oore runner register`. The CLI returns a one-time runner token that the runner process uses for all subsequent communication with the daemon.
- **Single-host operators** get a default embedded local runner when `oored` starts (`OORED_RUNNER_MODE=embedded`), so first builds do not require manual runner startup.
- **Runners** (once started via `oore runner start`) automatically poll for queued builds, report heartbeats, and execute claimed work. No manual job assignment is needed.
- **Developers** see builds progress from "queued" to "running" to a terminal state as runners claim and execute work.
- **Admins/Owners** can list all registered runners and monitor their status, capabilities, and last heartbeat via the API.

## UI Changes

Runner management now includes a dedicated admin UI page under Settings:

- `Settings -> Runners` lists all runners with status, heartbeat freshness, capabilities, registration source, and update time.
- Owners/Admins can rename externally registered runners from the UI.
- Embedded runners are shown but rename is disabled.

## API Changes

New endpoints:

- `POST /v1/runners/register` — Register a new runner. Requires admin or owner role. Returns a scoped runner token (plaintext, shown only once). Runner record is created with status `offline`.
- `POST /v1/runners/{runner_id}/heartbeat` — Runner heartbeat. Authenticated with runner token. Updates `last_heartbeat_at` and reports current capabilities (macOS version, Xcode version, available capacity). Transitions runner status to `online`.
- `POST /v1/runners/{runner_id}/claim` — Claim the next queued build. Authenticated with runner token. Uses a two-step optimistic locking transition (queued → scheduled → assigned) to prevent double-claims. Returns the build record and config snapshot on success, or `200 OK` with `{"job": null}` when the queue is empty.
- `POST /v1/runners/{runner_id}/jobs/{job_id}/status` — Report build execution status. Authenticated with runner token. Includes step results (name, status, exit_code, started_at, finished_at, duration_ms) and an overall exit_code. Step results and exit code are persisted on the build record. Runners can only set status to `running`, `succeeded`, or `failed`.
- `GET /v1/runners/{runner_id}/jobs/{job_id}` — Check current build status. Authenticated with runner token. Used by runners to poll for cancellation or timeout between build steps. Returns the build's current status string.
- `GET /v1/runners` — List all registered runners with status, capabilities, and last heartbeat. Requires admin or owner role.
- `PATCH /v1/runners/{runner_id}` — Rename runner. Requires admin or owner role. Accepts `{ "name": "<new_name>" }` and returns updated runner.

Runner token handling:

- Tokens are crypto-random (32 bytes, hex-encoded to 64 characters).
- SHA-256 hash of the token is stored in the `runners.token_hash` column.
- Plaintext token is returned exactly once at registration and never stored.
- Runner authenticates by sending the token in the `Authorization: Bearer <token>` header. The daemon hashes the provided token and matches against stored hashes.

Pull-based protocol: runners poll the claim endpoint on an interval (default 5 seconds). The daemon does not push work to runners. This aligns with the platform contract section 16 decision to use HTTPS JSON with pull-based scheduling.

Embedded runner mode:

- `oored` can bootstrap an embedded local runner at startup.
- Mode is controlled by `OORED_RUNNER_MODE` (`embedded` default, `external`, `hybrid`).
- Embedded mode keeps the same HTTP runner contract and auth checks; it does not introduce a separate private execution path.
- Embedded runners are daemon-managed (`registered_by IS NULL`) and cannot be renamed through the API (`409 embedded_runner_locked`).

## Security Considerations

- Runner tokens are crypto-random, hashed with SHA-256, and stored as hashes only. Plaintext is never persisted.
- Plaintext token is returned only once at registration time. If lost, the operator must re-register the runner.
- Cross-runner access is prevented: every runner-scoped endpoint validates that the `runner_id` in the URL path matches the runner authenticated by the bearer token. Mismatches return 403 Forbidden.
- Registration requires admin or owner role, preventing unauthorized runner enrollment.
- Rename requires admin or owner role and is audit logged (`runner_renamed`).
- A runner can only update builds it has claimed. The `runner_id` ownership check on the build record prevents runner A from reporting status for runner B's claimed build.
- Runner registration produces an `audit_logs` entry (action: `runner_registered`). Claim and status report transitions produce `build_events` entries via the build state machine. Heartbeats update runner state only (no audit record, to avoid log noise at polling frequency).
- Runner token scope is limited to runner operations — tokens cannot access user management, setup, or other daemon APIs.

## Migration and Rollout

The `runners` table was created in migration 005 (Phase 2) with all required columns: `id`, `name`, `token_hash`, `status`, `capabilities`, `last_heartbeat_at`, `created_at`, `updated_at`. Migration 006 adds `step_results` (TEXT) and `exit_code` (INTEGER) columns to the `builds` table for persisting runner-reported execution metadata. New HTTP endpoints are purely additive and do not affect existing API surfaces.

## Acceptance Criteria

- [ ] Runner can register via `oore runner register` and receive a one-time token
- [ ] Runner heartbeat updates status to online and records last_heartbeat_at
- [ ] Runner capability reporting captures macOS version and Xcode version
- [ ] Runner token authentication works on all runner-scoped endpoints
- [ ] Cross-runner access is blocked (runner A cannot heartbeat or claim as runner B)
- [ ] Admin can list all registered runners via GET /v1/runners
- [ ] Runner token is SHA-256 hashed before storage; plaintext is never persisted
- [ ] Pull-based claim returns 200 with `{"job": null}` when queue is empty

## Owner

oore.build team

## Last Updated

`2026-02-09`
