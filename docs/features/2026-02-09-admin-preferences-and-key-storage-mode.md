# Admin Preferences and Key Storage Mode

## Status

`ready`

## Problem

Admin-facing settings felt fragmented, with Artifact Storage as a standalone page.  
Also, there was no owner/admin control to choose encryption key startup source (Keychain vs legacy file), which made local operation preferences harder to manage.

## User Impact

Owner/Admin users now manage operational settings from a single `Preferences` page and can explicitly choose encryption key startup mode:

- `keychain` (recommended on macOS)
- `file` (legacy mode for prompt-free local workflows)

## UI Changes

- Sidebar admin section now includes `Preferences`.
- Artifact storage controls moved under `Settings -> Preferences`.
- Added a security preference toggle for encryption key storage mode.
- Legacy route `/settings/artifacts` redirects to `/settings/preferences`.

## API Changes

- Added `GET /v1/settings/preferences`
- Added `PUT /v1/settings/preferences`

New contract types:

- `KeyStorageMode` (`keychain | file`)
- `InstancePreferences`
- `InstancePreferencesResponse`
- `UpdateInstancePreferencesRequest`

Startup behavior:

- Daemon reads persisted `key_storage_mode` and loads encryption key from that source on startup.

## Security Considerations

- Changing key storage mode persists the currently active encryption key into the selected backend, preserving decryptability.
- Key mode updates require `instance_settings:write` permission (owner/admin).
- Preference updates are audit-logged (`instance_preferences_updated`).
- Mode changes are restart-sensitive by design; response includes `restart_required: true`.

## Migration and Rollout

- Added migration: `012_instance_preferences.sql`
- No destructive migration.
- Existing instances default to platform mode (`keychain` on macOS, `file` otherwise) until explicitly changed.

## Acceptance Criteria

- [x] Owner/Admin can open `Settings -> Preferences`.
- [x] Artifact storage settings are configurable from `Preferences`.
- [x] Owner/Admin can update key storage mode via UI and API.
- [x] Non-admin roles cannot modify preferences.
- [x] Daemon startup respects persisted key storage mode.

## Owner

Core backend + web app

## Last Updated

`2026-02-09`
