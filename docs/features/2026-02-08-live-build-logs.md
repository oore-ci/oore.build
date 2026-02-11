# Live Build Logs

## Status

`ready`

## Problem

Developers need real-time visibility into build execution progress. Without live logs, operators must wait for builds to complete before diagnosing issues. Build failures that occur midway through a long build pipeline are invisible until the entire build finishes, wasting time and slowing feedback loops. A live log streaming mechanism lets operators and developers monitor build output as it happens, enabling faster issue identification and reducing mean time to resolution.

## User Impact

- **Developers** can watch build output in real-time via the build detail page, gaining immediate visibility into compilation errors, test failures, and other issues as they occur.
- **QA engineers** can monitor build progress to know when artifacts will be ready for testing, without repeatedly refreshing the page.
- **Admins and operators** can diagnose infrastructure problems (runner disk full, Xcode license expired, signing failures) in real time instead of waiting for a post-mortem.
- Log history is preserved for completed builds, so all users can review full build output after completion.

## UI Changes

The build detail page now includes a live log viewer component:

- **Log viewer panel** with a terminal-like dark background, monospace font, and auto-scroll behavior. Auto-scroll is active by default when viewing a running build and pauses when the user scrolls up.
- **Streaming indicator** (pulsing dot or similar visual cue) shows when logs are live-streaming from an in-progress build. The indicator disappears when the build reaches a terminal state.
- **Step-oriented log navigation** shows build steps (`checkout`, `pre_build-*`, `build-*`, `post_build-*`) as selectable controls, with per-step status badges and a "current step" hint while running.
- **stdout/stderr differentiation** — stderr lines are visually distinct (e.g., red or orange text) to help users quickly identify warnings and errors.
- **Scroll-to-bottom button** appears when auto-scroll is paused, allowing users to jump back to the latest log output.
- For completed builds, the log viewer displays the full persisted log history with the same formatting.
- **Artifacts auto-refresh** keeps the artifacts panel updated during active builds and refreshes immediately when stream completion is detected.

## API Changes

New endpoints:

- `POST /v1/runners/{runner_id}/jobs/{job_id}/logs` — Runner uploads ordered log chunks during build execution. Request body is JSON with a `chunks` array, where each chunk includes `sequence` (integer), `content` (string), and `stream` (`"stdout"` or `"stderr"`). Authenticated with runner token (RunnerAuth). Returns JSON `{ "appended": <count> }` on success.
- `GET /v1/builds/{build_id}/logs` — Retrieve build logs with pagination. Accepts `after_sequence` (integer, returns entries with sequence > value) and `limit` (integer, max 5000, default 1000) query parameters. Returns `{ "logs": [...], "total": <count> }` ordered by sequence. Requires AuthUser with RBAC permission `builds:read`.
- `POST /v1/builds/{build_id}/stream-token` — Exchange a session token for a short-lived (5-minute TTL) streaming token. The streaming token is scoped and stored in-memory on the server. This prevents the long-lived session token from appearing in URL query strings. Requires AuthUser with RBAC permission `builds:read`. Returns `{ "token": "...", "expires_at": <unix_timestamp> }`.
- `GET /v1/builds/{build_id}/logs/stream` — SSE (Server-Sent Events) endpoint for live log streaming. Each SSE event has type `log` with JSON data containing `sequence`, `content`, and `stream` fields. A `done` event is sent when the build reaches a terminal state. Supports reconnection via `Last-Event-ID` header — the server resumes streaming from the sequence number after the provided ID. Authentication is source-aware: the `?token=` query parameter accepts **only** short-lived streaming tokens (full session tokens are rejected); the `Authorization: Bearer` header accepts full session tokens for non-browser clients. Requires RBAC permission `builds:read`.
- Runner log stream now includes structured step marker lines in normal log chunks (prefix: `[oore-step]`) to represent step start/end boundaries for UI segmentation.

New database schema:

- Migration 007 adds the `build_logs` table with columns: `id` (TEXT PRIMARY KEY), `build_id` (TEXT NOT NULL, FK to builds with ON DELETE CASCADE), `sequence` (INTEGER NOT NULL), `content` (TEXT NOT NULL), `stream` (TEXT NOT NULL, CHECK constraint for "stdout" or "stderr", default "stdout"), `created_at` (INTEGER NOT NULL). A UNIQUE constraint on `(build_id, sequence)` ensures ordering and prevents duplicates, with an index on `(build_id, sequence)` for efficient querying.

## Security Considerations

- Log content is only accessible to authenticated users with the `builds:read` RBAC permission. Unauthenticated requests receive 401 Unauthorized.
- The SSE stream endpoint authenticates via a `?token=` query parameter because the browser `EventSource` API does not support setting custom headers. The frontend first exchanges the session token for a short-lived (5-minute) streaming token via `POST /v1/builds/{build_id}/stream-token`, so the long-lived session token never appears in URL query strings. The streaming tokens are stored in-memory (hashed with SHA-256) and expire automatically. The `?token=` parameter enforces strict source separation: it accepts **only** short-lived stream tokens and rejects full session tokens, preventing accidental session token exposure in URLs. Non-browser clients (curl, CI scripts) can authenticate via the `Authorization: Bearer` header with a full session token.
- Log lines are truncated at 4 KB per entry to prevent memory abuse from pathologically long output lines.
- A maximum of 10,000 log lines per build prevents storage abuse. Once the limit is reached, the server silently drops new chunks and returns `{ "appended": 0 }`. Excess chunks are logged server-side as warnings.
- Runner log upload is authenticated via the runner token. The runner can only upload logs for builds it has claimed (runner_id ownership check on the build record).
- Log content may inadvertently contain secrets (e.g., environment variables echoed by build scripts). A future enhancement may add secret masking, but in V1, operators are responsible for ensuring build scripts do not echo secrets.

## Migration and Rollout

Migration 007 adds the `build_logs` table. No data migration is needed — existing builds simply have no log entries. The feature is available immediately after the migration runs on daemon startup. No feature flags or gradual rollout are required.

SSE streaming requires no additional infrastructure beyond the existing Axum HTTP server. Tokio's async runtime handles concurrent SSE connections efficiently.

Runtime performance tuning (2026-02-11):

- Frontend polling reconciliation now pauses while SSE is healthy and resumes only on disconnect/fallback, avoiding duplicate fetch pressure during active streams.
- Backend SSE polling cadence is relaxed to 1 second, and build terminal-status checks are performed every 2 seconds instead of every stream tick.

## Acceptance Criteria

- [x] Runner can upload ordered log chunks during build execution
- [x] Build detail page shows live-updating logs during running builds
- [x] Build detail page shows current running step and per-step log segmentation
- [x] Completed build logs are retrievable via GET endpoint with pagination
- [x] SSE stream supports reconnection via Last-Event-ID
- [x] Polling reconciliation keeps log view updated when SSE is interrupted
- [x] Log truncation safeguards prevent abuse (4 KB per line, 10,000 lines per build)
- [x] Logs are ordered by sequence number
- [x] stdout and stderr streams are visually differentiated in the UI

## Owner

Platform team

## Last Updated

`2026-02-11`
