# Documentation Policy

## Rules

- Documentation is required for every user-facing feature.
- Feature docs must live under `docs/features/`.
- Feature docs must follow the required template sections.
- Pull requests that change product code must include feature docs updates.
- Pull requests that change finalized platform decisions must update `docs/platform-contract.md`.
- Breaking or redefining a `MUST` rule requires an ADR plus matching contract and feature-doc updates.
- The static docs site uses `VitePress` from `apps/docs-site/docs`.
- The static docs site (`apps/docs-site`) must stay deployable as part of release workflows.

## Required Section Headers

- `## Status`
- `## Problem`
- `## User Impact`
- `## UI Changes`
- `## API Changes`
- `## Security Considerations`
- `## Migration and Rollout`
- `## Acceptance Criteria`
- `## Owner`
- `## Last Updated`
