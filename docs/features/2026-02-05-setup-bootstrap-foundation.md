# Setup Bootstrap Foundation

## Status

`ready`

## Problem

oore.build instances need a secure, contract-compliant setup flow to go from a fresh install to a fully configured state. The setup must enforce one-time bootstrap tokens with TTL, gate mutating setup endpoints with session auth, and auto-disable setup after reaching `ready` state. Without this, there is no safe way to initialize the platform.

## User Impact

Operators can now run `oore setup open --ttl <duration>` to generate a time-bound bootstrap token, then use the web UI or API to complete setup (configure OIDC, create owner, finalize). The setup flow enforces security invariants automatically: tokens are one-time use, sessions expire after 30 minutes, and all setup endpoints are permanently disabled once the instance reaches `ready` state.

## UI Changes

No UI changes in this slice. The setup flow is API-driven. The hosted web UI will consume these endpoints in a future slice.

## API Changes

- `GET /v1/public/setup-status` — now sourced from persisted file-backed state (previously env var placeholder). Returns `instance_id`, `state`, `setup_mode`, `is_configured`. Non-sensitive.
- `POST /v1/setup/bootstrap-token/verify` — accepts `{ "token": "..." }`, validates against stored hash/expiry/consumed status, returns `{ "session_token": "...", "expires_at": <epoch> }`. One-time use.
- `POST /v1/setup/oidc/configure` — requires Bearer session token. Accepts `{ "issuer_url", "client_id", "client_secret?" }`. Transitions from `bootstrap_pending` to `idp_configured`.
- `POST /v1/setup/owner/finalize` — requires Bearer session token. Accepts `{ "owner_email" }`. Transitions from `idp_configured` to `owner_created`.
- `POST /v1/setup/complete` — requires Bearer session token. Transitions from `owner_created` to `ready`. Auto-disables all setup endpoints.
- All setup mutating endpoints return `409` with structured `ApiError` after state is `ready`.

## Security Considerations

- Bootstrap token is generated with 32 bytes of cryptographic randomness (OsRng), stored as SHA-256 hash only — plaintext is never persisted.
- Bootstrap token is one-time: consumed on verify, rejected if reused.
- Bootstrap token is TTL-bound: rejected if expired.
- Setup session token is generated fresh after bootstrap verify, stored as SHA-256 hash, expires after 30 minutes.
- All setup mutating endpoints require valid session auth (Bearer token in Authorization header).
- Public setup status endpoint exposes only non-sensitive progress state — no tokens, secrets, or credentials.
- All setup endpoints are permanently disabled once state reaches `ready`.
- State file is written atomically (write-to-tmp + rename) to prevent corruption.

## Migration and Rollout

First implementation — no migration needed. State file is created fresh on first run. Existing dev instances can be re-initialized by deleting the state file at `~/Library/Application Support/oore/setup-state.json`.

## Acceptance Criteria

- [x] `oore setup open --ttl <duration>` generates token with file-backed persistence
- [x] `oore setup open --json` outputs machine-readable JSON
- [x] Bootstrap token is one-time and TTL-bound
- [x] Setup session auth gates all setup mutating endpoints
- [x] Setup mutating endpoints return 409 when state is `ready`
- [x] `GET /v1/public/setup-status` remains non-sensitive
- [x] State transitions enforce correct ordering (bootstrap_pending → idp_configured → owner_created → ready)
- [x] Invalid transitions return 409 with structured error
- [x] State file persists across daemon restarts
- [x] Feature documentation passes docs gate

## Owner

Platform team

## Last Updated

`2026-02-06`
