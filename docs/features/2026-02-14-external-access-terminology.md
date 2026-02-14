# External Access Terminology in UI

## Status

`implemented`

## Problem

User-facing copy used "remote mode", which reads as internal implementation language and is unclear during onboarding/settings.

## User Impact

- Users now see clearer wording:
  - `Local Only`
  - `External Access`
- Integration setup blockers and requirements are easier to understand without backend jargon.

## UI Changes

- Replaced user-visible `Remote Mode Required` with `External Access Required`.
- Updated disabled-state guidance in GitHub/GitLab setup pages to reference `Local Only` and enabling `External Access`.
- Updated Sources index blocker copy and alert copy to use `External Access` terminology.
- Updated Local Repositories helper copy to reference `Local Only` and `External Access`.
- Updated login sign-in method label from `Local Mode` to `Local Only`.
- Updated setup owner mode-restricted message to `Local Only` terminology.

## API Changes

- None.
- Runtime mode values remain unchanged in API/contract (`local` / `remote`).

## Security Considerations

- Copy-only change; no auth or permission behavior changes.
- Existing enforcement for mode-restricted flows remains unchanged.

## Migration and Rollout

1. Deploy web app update.
2. No backend or data migration required.

## Acceptance Criteria

- [x] User-visible "remote mode" wording is replaced with `External Access` in settings/integrations flows.
- [x] User-visible local mode wording is normalized to `Local Only` where updated.
- [x] Internal API/runtime fields remain unchanged.

## Owner

Platform team

## Last Updated

`2026-02-14`
