# External Access Security Hardening (Loopback Boundary + Hard Preflight)

## Status

`ready`

## Problem

Local-first onboarding reduced setup friction, but non-loopback access paths
were not fail-closed enough for alpha hardening. Local sign-in needed a strict
loopback boundary, and External Access needed deterministic preflight gating
before enabling remote exposure.

## User Impact

- Local same-machine usage remains fast: loopback local sign-in still works.
- LAN/Tailscale/`.local` usage in `local` mode is explicitly blocked for local
  sign-in.
- Operators get a clear External Access readiness panel and actionable failing
  checks before enablement.
- Runtime mode changes force re-auth by revoking active sessions.

## UI Changes

- Dashboard auto local sign-in now runs only when both UI host and backend host
  are loopback.
- Login page blocks local-mode sign-in on non-loopback hosts and points users
  to enabling External Access on the host machine.
- Preferences now includes:
  - External Access status (`Local Only` vs `External Access`)
  - owner-only toggle for runtime mode
  - readiness card with check-by-check pass/fail + remediation guidance

## API Changes

- Added `GET /v1/settings/external-access/preflight`.
- Updated `PUT /v1/settings/preferences` behavior:
  - runtime mode mutation is owner-only
  - enabling `remote` requires preflight pass
  - mode change revokes all sessions
- Updated `POST /v1/auth/local/login`:
  - rejects non-loopback clients with `403 local_login_loopback_required`

New error codes used in this increment:

- `local_login_loopback_required`
- `external_access_preflight_failed`
- `external_access_https_required`
- `external_access_owner_required`
- `external_access_origin_not_allowed`
- `external_access_public_url_missing`

Contract type additions:

- `ExternalAccessPreflightResponse`
- `ExternalAccessPreflightCheck`

## Security Considerations

- Local auth trust boundary is enforced with peer-address loopback checks from
  Axum `ConnectInfo`.
- External Access enablement is fail-closed on core security checks.
- Session revocation on runtime mode switches prevents policy drift from stale
  tokens.
- CORS defaults were tightened to local launcher/dev origins; hosted/remote
  origins are opt-in via environment configuration.

## Migration and Rollout

- No schema migration required.
- Existing local flows continue on loopback.
- Operators enabling External Access must configure:
  - `OORE_PUBLIC_URL` as non-loopback HTTPS
  - `OORE_CORS_ORIGINS` including the public origin
  - valid OIDC config
- OpenAPI and settings/auth reference docs updated in the same increment.

## Acceptance Criteria

- [x] Non-loopback local login is rejected with deterministic error code.
- [x] External Access enablement fails unless all hard preflight checks pass.
- [x] Runtime mode mutation is owner-only.
- [x] Runtime mode change revokes all active sessions.
- [x] Loopback local flow remains frictionless (auto-login/autobootstrap).
- [x] UI surfaces External Access readiness and owner-only toggle behavior.

## Owner

oore.build core platform team

## Last Updated

`2026-02-14`
