# Connected Sources Tile Interaction

## Status

`implemented`

## Problem

The Connected Sources section used a table with a dedicated `Open` button, which made the primary action feel indirect and visually heavy.

## User Impact

- Users can open source details by clicking anywhere on the source tile.
- Hover and focus states make the primary interaction clearer.
- Secondary destructive action (`Disconnect`) remains explicit and separate.

## UI Changes

- Replaced the Connected Sources table with a tile list in `/settings/integrations`.
- Each tile now includes:
  - Name and short ID
  - Provider/status/auth mode badges
  - Scope badge (`Single repo` for `local_git`, `Multi repo` for hosted providers)
  - Host URL
- Tile click navigates to source details.
- `Disconnect` remains a secondary action button with confirmation dialog.

## API Changes

- None.

## Security Considerations

- No change to authorization or backend behavior.
- Existing permission checks remain unchanged.

## Migration and Rollout

1. Deploy frontend update.
2. No backend or database migration required.

## Acceptance Criteria

- [x] Connected Sources uses tile-based presentation.
- [x] Primary action is tile click (no dedicated `Open` button).
- [x] Disconnect flow remains available with confirmation.

## Owner

Platform team

## Last Updated

`2026-02-14`
