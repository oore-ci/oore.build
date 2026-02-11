# iOS Ad Hoc Signing (Pipeline-Scoped)

## Status

`ready`

## Problem

Teams building iOS ad hoc artifacts needed a secure signing workflow equivalent to Android signing, but without relying on repository secrets or host-level environment fallback. They also needed a guided way to register devices and regenerate provisioning assets from UI/API flows.

## User Impact

Owners/admins/developers can now configure iOS signing per pipeline in three modes:

- `manual`: upload `.p12` + provisioning profiles.
- `api`: use App Store Connect API credentials and sync automation.
- `hybrid`: manual certificate with API-driven device/profile operations.

Runners receive a unified, decrypted signing bundle only for assigned jobs and produce signed IPA artifacts with signing provenance metadata.

## UI Changes

- Pipeline create/edit form includes a new **iOS Signing** section with guided fields:
  - mode selection (`manual|api|hybrid`)
  - Team ID
  - bundle ID mapping input
  - `.p12` upload and password (manual/hybrid)
  - App Store Connect API key metadata + `.p8` upload (api/hybrid)
  - provisioning profile uploads per bundle ID (manual/hybrid)
- Pipeline edit page includes **Registered iOS Devices** panel:
  - list cached devices
  - register device (name + UDID)
  - trigger profile sync/regeneration

## API Changes

Added pipeline-scoped iOS signing endpoints:

- `GET /v1/pipelines/{pipeline_id}/ios-signing`
- `PUT /v1/pipelines/{pipeline_id}/ios-signing`
- `POST /v1/pipelines/{pipeline_id}/ios-signing/sync`
- `GET /v1/pipelines/{pipeline_id}/ios-signing/devices`
- `POST /v1/pipelines/{pipeline_id}/ios-signing/devices/register`
- `GET /v1/runners/{runner_id}/jobs/{job_id}/ios-signing` (runner-auth only)

Added contract payloads for mode/config updates, public-safe metadata responses, device registration/sync responses, and runner signing bundle retrieval.

## Security Considerations

- iOS signing secrets (`.p12`, p12 password, `.p8`) are encrypted at rest using instance encryption key.
- No plaintext signing secrets are returned by pipeline read endpoints.
- Runner signing bundle endpoint is job-assignment constrained (runner token must match assigned runner).
- Signing material is only decrypted server-side when building runner payloads or API automation workflows.
- Runner uses ephemeral keychain/profile materialization and cleanup to avoid persistence on workspace snapshots.
- Device registration/sync and signing updates emit audit events.

## Migration and Rollout

- DB migration `014_pipeline_ios_signing.sql` adds settings/profiles/devices tables.
- Rollout target is feature-complete V1 iOS ad hoc signing (manual + api + hybrid + device registration) behind backend/frontend support.
- Existing pipelines are unaffected until iOS signing is explicitly configured.

## Acceptance Criteria

- [x] Pipeline-scoped iOS signing settings CRUD supports manual/api/hybrid modes.
- [x] Runner can fetch authorized iOS signing bundle and prepare ephemeral signing material.
- [x] UI supports iOS signing configuration, device registration, and profile sync trigger.
- [x] Sensitive iOS signing assets are encrypted at rest and excluded from public API responses.

## Owner

Platform / Runtime

## Last Updated

`2026-02-11`
