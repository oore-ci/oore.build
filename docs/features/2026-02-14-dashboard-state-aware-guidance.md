# Dashboard State-Aware Guidance and CTA Gating

## Status

`implemented`

## Problem

The dashboard showed generic CTAs (`Run Build`, `Create project`) even when prerequisites were missing, especially when zero sources were configured. This created dead-end actions during first-run onboarding.

## User Impact

- Dashboard remains simple and section-first (Projects + Recent Builds) without a separate onboarding stage.
- Users can create a project directly from the Projects empty state in one click (opens the create dialog immediately).
- Builds section remains visible with a simple empty state until builds exist.

## UI Changes

- Removed `Setup Progress` stage from dashboard.
- Projects section empty state now drives first action:
  - if source missing (remote mode), show `Connect Source`
  - otherwise show `Create Project` (deep-links to Projects page with dialog pre-opened)
- Recent Builds section now uses a plain empty state (`No builds yet.`) until build history exists.
- Header `Run Build` action remains gated by prerequisites (`has project`, source ready where required, and permission).

## API Changes

- None.
- Existing `GET /v1/integrations`, `GET /v1/projects`, and `GET /v1/builds` responses are used to derive dashboard readiness state (shown as source readiness in UI).

## Security Considerations

- UI actions are permission-gated using existing client-side RBAC checks (`integrations:write`, `projects:write`, `builds:write`).
- No new auth flows, tokens, or data exposure were introduced.

## Migration and Rollout

1. Deploy web app update.
2. No backend migration required.
3. Existing configured instances automatically see state-aware dashboard behavior.

## Acceptance Criteria

- [x] Dashboard has no dedicated `Setup Progress` module.
- [x] Projects section empty state provides the relevant next action and opens project creation in a single click.
- [x] Recent Builds section shows a simple empty state when there are no builds.
- [x] Dashboard actions remain role/prerequisite aware.

## Owner

Platform team

## Last Updated

`2026-02-14`
