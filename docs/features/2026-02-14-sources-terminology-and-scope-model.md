# Sources Terminology and Scope Model Alignment

## Status

`implemented`

## Problem

UI terminology used "Integrations" everywhere, while local mode behavior is single-repository oriented. This created a conceptual mismatch: hosted providers represent multi-repository connections, but local setup often represents one repository at a time.

## User Impact

- Users now see a clearer top-level concept: **Sources**.
- Scope is explicit in connected source tiles:
  - `Single repo` for local repository sources.
  - `Multi repo` for GitHub/GitLab sources.
- First-run guidance uses source-centric language (`Connect Source`, `Manage Sources`).

## UI Changes

- Renamed primary navigation/admin label from `Integrations` to `Sources`.
- Updated sources management header and connected list copy to source-centric language.
- Updated dashboard/projects/builds CTA and guidance copy from integration-centric wording to source-centric wording.
- Updated back-navigation labels on source setup/detail pages to `Sources`.
- Added scope badge to connected source tiles.

## API Changes

- None.
- Backend API paths and contracts remain unchanged (`/v1/integrations/*`).

## Security Considerations

- No auth, RBAC, or token behavior changes.
- This is a presentation/model clarity update only.

## Migration and Rollout

1. Deploy frontend update.
2. No backend or database migration required.
3. Existing integrations appear as sources automatically.

## Acceptance Criteria

- [x] Source-facing UI labels replace integration-facing labels in onboarding and settings navigation.
- [x] Connected source list includes `Single repo` / `Multi repo` scope cues.
- [x] Route paths and backend API contracts remain stable.

## Owner

Platform team

## Last Updated

`2026-02-14`
