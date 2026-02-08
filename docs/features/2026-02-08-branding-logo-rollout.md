# Project Logo Rollout Across Web and Docs Surfaces

## Status

`ready`

## Problem

The frontend surfaces used placeholder branding artifacts and inconsistent icon assets across browser tabs, app manifests, and visible headers. This caused mismatched identity between the main web UI and the docs site.

## User Impact

Users now see a consistent oore.build brand mark in key touchpoints:
- browser tab/favicon
- app install metadata (manifest icons)
- web UI authenticated header and setup/login entry views
- docs site SPA header and VitePress nav logo
- backend-served OAuth/integration HTML routes (GitHub/GitLab and CLI callback pages)

## UI Changes

- Added canonical `logo.svg` brand asset to both frontends.
- Replaced favicon and app icon rasters with regenerated assets derived from the finalized vector logo.
- Updated web app header to display logo + wordmark in authenticated layout.
- Updated setup and login pages to include the finalized logo mark.
- Replaced docs-site SPA header logo with the finalized logo.
- Configured VitePress docs nav to use the finalized logo.
- Added favicon metadata to backend-rendered HTML pages served from Rust handlers.

## API Changes

None.

## Security Considerations

No security model changes. This is a branding and static-asset update only.

## Migration and Rollout

- No backend migration required.
- Existing clients will pick up new icons after cache refresh/deploy.
- Added SVG icon entries in manifests while retaining `.ico` fallback.

## Acceptance Criteria

- [x] Finalized logo is present in web app browser metadata and manifest icons
- [x] Finalized logo is present in docs site browser metadata and manifest icons
- [x] Web app header/login/setup surfaces show updated logo
- [x] Docs SPA header and VitePress nav show updated logo
- [x] Backend HTML routes include the finalized favicon
- [x] Icon rasters (`logo192.png`, `logo512.png`, `favicon.ico`) regenerated from finalized SVG

## Owner

Platform team

## Last Updated

`2026-02-08`
