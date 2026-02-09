# Artifact Storage Settings UI and Backends

## Status

`released`

## Problem

Artifact metadata could be recorded without object storage, but binary download links required environment-based S3 configuration. This forced operators to restart/configure the daemon via env vars and made local-only setups hard to operate. There was no owner/admin UI for storage backend control.

## User Impact

- **Owners/Admins** can configure artifact storage from the product UI without manual env-var editing and restarts.
- **Developers/QA** can download artifacts when local storage is selected, even without S3/R2.
- Teams can choose backend strategy per instance: disabled, local filesystem, S3, or R2.

## UI Changes

- Added **Settings -> Artifact Storage** page in `apps/web`.
- Owner/Admin can:
  - choose provider: `disabled`, `local`, `s3`, `r2`
  - configure local base directory (absolute path)
  - configure S3/R2 bucket/region/endpoint
  - set or rotate access key + secret key
- Credential fields are write-only in UI; existing credentials are represented as stored/missing flags.
- Provider/source summary cards show active mode and config source.

## API Changes

New endpoint:

- `GET /v1/settings/artifact-storage`
  - returns current effective settings (database/env/default source) without exposing secrets
- `PUT /v1/settings/artifact-storage`
  - updates persisted settings for provider/backends
  - owner/admin gated (`instance_settings:write`)
  - encrypts credentials at rest
  - hot-reloads artifact backend in-process

New local signed transfer endpoints (used by generated links/tokens):

- `PUT /v1/artifacts/local-upload/{token}`
- `GET /v1/artifacts/local-download/{token}`

Contract additions in `oore-contract`:

- `ArtifactStorageProvider`
- `ArtifactStorageSource`
- `ArtifactStorageSettings`
- `ArtifactStorageSettingsResponse`
- `UpdateArtifactStorageSettingsRequest`

## Security Considerations

- S3/R2 credentials are encrypted with existing AES-256-GCM daemon key before persistence.
- Secrets are never returned by read APIs.
- Storage settings updates are role-gated and audit logged (`artifact_storage_updated`).
- Local upload/download URLs are short-lived signed tokens.
- Artifact names reject path separators, reducing traversal risk for local storage keys.

## Migration and Rollout

- Migration `011_artifact_storage_settings.sql` adds persisted storage settings.
- If DB settings are absent, daemon continues env fallback behavior for S3 config.
- Local backend can be enabled incrementally from UI with no daemon restart.

## Acceptance Criteria

- [x] Owner/Admin can read and update artifact storage settings from UI.
- [x] Credentials are stored encrypted and never exposed in API responses.
- [x] Local storage backend supports artifact upload + download link flow.
- [x] Runtime backend switch applies immediately after settings update.
- [x] Existing env fallback remains available when DB settings are not configured.

## Owner

Platform team

## Last Updated

`2026-02-09`
