# Projects and Builds State-Aware Onboarding Gating

## Status

`implemented`

## Problem

`/projects` and `/builds` showed generic CTAs regardless of setup readiness. This produced dead-end flows (for example, showing build actions before source/project prerequisites existed) and repeated prompts.

## User Impact

- Users now see the correct next step based on readiness.
- Invalid actions are no longer promoted before prerequisites exist.
- Pages are less repetitive, with one primary action per state.

## UI Changes

- `/projects` now has conditional states:
  - `connect source first` state when no active source exists.
  - `create first project` state after source is connected.
  - project inventory table only when projects exist.
- `/builds` now has conditional states:
  - `connect source first` when no active source exists.
  - `create project first` when source exists but no projects exist.
  - build history table when prerequisites exist and builds are present.
  - focused empty state with `Trigger first build` only when prerequisites are met.
- Actions are permission-aware (`integrations:write`, `projects:write`, `builds:write`) and show clear guidance when the current user cannot perform the step.
- Runtime-aware integration routing is applied (`local` -> Local Git path).

## API Changes

- None.
- Existing integrations/projects/builds queries are reused to derive UI state.

## Security Considerations

- No backend auth or permission model changes.
- UI gating remains advisory; backend authorization remains source of truth.

## Migration and Rollout

1. Deploy web update.
2. No data migration required.
3. Existing instances automatically receive conditional page behavior.

## Acceptance Criteria

- [x] `/projects` does not show project creation flow before a source is active.
- [x] `/builds` does not show build-trigger flow before source and project prerequisites are met.
- [x] Conditional states avoid redundant CTA surfaces.
- [x] Role-limited users see clear non-actionable guidance instead of invalid action buttons.

## Owner

Platform team

## Last Updated

`2026-02-14`
