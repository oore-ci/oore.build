# External Access Readiness Actions in Settings

## Status

`ready`

## Problem

The Preferences view for External Access had become overly verbose and
diagnostic-heavy. Owners saw repeated messages/actions across multiple
sections, weak sequencing, and unclear next steps for enablement.

## User Impact

- Owners now get a clear sequence: configure network, configure OIDC, enable.
- The primary action (`Turn On External Access`) stays visible and gated by
  readiness state.
- Detailed preflight diagnostics remain available but are collapsed by
  default, reducing cognitive load.

## UI Changes

- `apps/web/src/routes/settings/preferences.tsx`
  - Replaced dense readiness-first layout with a guided task flow.
  - Added step cards for Network and Identity configuration.
  - Kept a single primary enable/disable action in the current-access header.
  - Moved network settings into a dialog (parallel to OIDC dialog) to keep
    the page concise.
  - Kept preflight checks behind a collapsible `Technical checks` panel.

## API Changes

- Added `PUT /v1/settings/external-access/oidc`
  - Owner-only.
  - Request: `ConfigureExternalAccessOidcRequest`
  - Response: `ConfigureExternalAccessOidcResponse`
  - Performs OIDC provider discovery and persists runtime OIDC settings.
- Existing External Access preflight endpoint unchanged:
  - `GET /v1/settings/external-access/preflight`

## Security Considerations

- New OIDC config endpoint is explicitly owner-only and still protected by
  `instance_settings:write` permission checks.
- Endpoint is state-gated to `setup_state == ready` to avoid setup-flow
  policy conflicts.
- Client secret is encrypted at rest using the existing encryption key path.
- Privileged changes are audit logged (`external_access_oidc_configured`).
- External Access enablement remains separate and fail-closed behind preflight
  checks and owner-only mode mutation.

## Migration and Rollout

- No schema migration required.
- No breaking API changes.
- OpenAPI spec regenerated to include the new endpoint and types.
- Settings API docs updated with request/response/error contract.

## Acceptance Criteria

- [x] Owner can configure OIDC for External Access from Preferences after setup is `ready`.
- [x] Owner can configure External Access network settings in-product from Preferences.
- [x] Non-owner receives `external_access_owner_required` when calling the new endpoint.
- [x] External Access UI provides a guided step sequence with direct actions.
- [x] Preflight diagnostics remain available without dominating the main flow.
- [x] External Access enablement behavior remains fail-closed and owner-only.

## Owner

Platform team

## Last Updated

`2026-02-14`
