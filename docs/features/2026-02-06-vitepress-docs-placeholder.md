# VitePress Docs Placeholder

## Status

`ready`

## Problem

The docs site framework decision was made but not reflected in repository tooling or docs-site scaffolding.

## User Impact

Contributors now have a clear docs framework (`VitePress`) and runnable placeholder docs site commands.

## UI Changes

No product UI changes. Added documentation-site placeholder pages.

## API Changes

No API changes.

## Security Considerations

No auth or security behavior changes.

## Migration and Rollout

No migration required. Use `bun run dev:docs` and `bun run build:docs` for docs site workflows.

## Acceptance Criteria

- [x] `apps/docs-site` includes VitePress dependency and scripts.
- [x] Placeholder VitePress config and pages exist under `apps/docs-site/docs`.
- [x] Contract and policy docs explicitly declare VitePress as docs framework.

## Owner

Docs platform

## Last Updated

`2026-02-06`
