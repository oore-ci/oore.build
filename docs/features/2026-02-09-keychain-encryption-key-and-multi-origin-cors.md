# Keychain Encryption Key and Multi-Origin CORS

## Status

`ready`

## Problem

Secret encryption keys were file-backed only, which is weaker than platform-native secret storage on macOS.  
Also, CORS origin handling supported only one origin, which blocked mixed local + hosted UI use cases.

## User Impact

Operators get stronger default key custody on macOS and can use both local UI and hosted UI without CORS failures.

## UI Changes

No direct UI surface changes.

## API Changes

No endpoint shape changes.

Runtime behavior changes:

- CORS now allows multiple approved origins.
- `PUT` is included in allowed CORS methods.

Configuration:

- `OORE_CORS_ORIGINS` (comma-separated, preferred)
- `OORE_CORS_ORIGIN` (single origin, backward-compatible fallback)

## Security Considerations

- On macOS, encryption key loading now prefers Keychain (`service=build.oore.oored`, `account=encryption-key-v1`).
- Existing legacy file key (`~/Library/Application Support/oore/encryption.key`) is migrated to Keychain when available.
- If Keychain operations fail, runtime falls back to legacy file behavior to avoid startup outages.
- Secrets remain encrypted at rest with AES-256-GCM; only key custody changed.

## Migration and Rollout

- No DB migration required.
- Existing instances auto-migrate legacy file key material to Keychain on startup (best effort).
- No API client updates required.

## Acceptance Criteria

- [x] Daemon starts using Keychain-backed encryption key on macOS.
- [x] Existing legacy file key is migrated to Keychain when present.
- [x] Fallback to legacy file key works if Keychain interaction fails.
- [x] CORS allows both local and hosted approved origins.
- [x] CORS preflight for `PUT` endpoints succeeds.

## Owner

Core backend

## Last Updated

`2026-02-09`
