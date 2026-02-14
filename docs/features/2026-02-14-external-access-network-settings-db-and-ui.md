# External Access Network Settings in DB + UI

## Status

`ready`

## Problem

External Access readiness depended on environment variables (`OORE_PUBLIC_URL`,
`OORE_CORS_ORIGINS`) that had to be edited manually on the host machine. This
created setup friction and made remediation from the UI incomplete.

## User Impact

- Owners can now configure External Access Public URL and allowed frontend
  origins directly in Preferences.
- Readiness failures are now actionable from the same page, without editing
  `.env` files.
- Existing env-based deployments continue to work as fallback until DB settings
  are written.

## UI Changes

- Preferences now includes an `External Access network settings` editor:
  - Public URL (HTTPS, non-loopback)
  - Allowed frontend origins (line/comma separated)
- The network editor is collapsed by default and can be opened on demand.
- Readiness check rows now route users to:
  - `Configure OIDC` for OIDC failures
  - `Edit network settings` for URL/origin/redirect consistency failures
- Login/setup messaging now references Preferences configuration rather than
  manual env variable edits.

## API Changes

- Added `GET /v1/settings/external-access/network`
- Added `PUT /v1/settings/external-access/network`
- Added contract types:
  - `ExternalAccessNetworkSource`
  - `ExternalAccessNetworkSettings`
  - `ExternalAccessNetworkSettingsResponse`
  - `UpdateExternalAccessNetworkSettingsRequest`
- Runtime consumers now use effective network settings from shared runtime state
  instead of reading env directly:
  - External Access preflight
  - OIDC redirect validation callers
  - GitHub/GitLab redirect-origin checks
  - GitLab OAuth callback URL generation
  - Local artifact signed URL base generation

## Security Considerations

- `PUT /v1/settings/external-access/network` is owner-only.
- In `local` mode, network settings updates are loopback-only.
- Public URL validation enforces:
  - valid URL
  - no embedded credentials
  - HTTPS required
  - non-loopback host required
- Public URL origin must be present in `allowed_origins`.
- Local defaults (`localhost` + `127.0.0.1` frontend origins) are always
  retained to avoid accidental lockout of local operator access.

## Migration and Rollout

- Added SQLite migration:
  - `017_external_access_network_settings.sql`
- No breaking migration for existing installations:
  - if DB settings row is missing, runtime falls back to env/default values.
- OpenAPI was updated and regenerated.

## Acceptance Criteria

- [x] Owners can read/update External Access network settings from API and UI.
- [x] Local mode blocks non-loopback network-settings updates.
- [x] External Access readiness and integration callbacks use DB-backed runtime
  network settings.
- [x] Existing env-based configuration still works as fallback when DB row is
  absent.

## Owner

Platform team

## Last Updated

`2026-02-14`
