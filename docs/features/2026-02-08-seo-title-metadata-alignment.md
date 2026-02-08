# SEO Title and Metadata Alignment for Web and Docs

## Status

`ready`

## Problem

Primary page titles defaulted to `oore.build` in several user-facing routes, reducing clarity in search results and browser tabs. Some key pages also lacked route-level title updates and social metadata consistency.

## User Impact

Users and search engines now see clearer page intent and product naming:
- consistent `Page | Oore CI` route titles in the web app
- stronger default titles for the web app and docs site
- aligned Open Graph and Twitter metadata for richer link previews

## UI Changes

- Added a shared web title helper and updated page titles across dashboard, setup, login, builds, users, and integrations routes.
- Updated web app base title from `oore.build` to `Oore CI | Self-Hosted Mobile CI`.
- Updated docs site/VitePress branding titles to `Oore CI Docs`.
- Updated VitePress home hero brand label to `Oore CI`.
- Updated web/docs manifest app names to `Oore CI` and `Oore CI Docs`.

## API Changes

None.

## Security Considerations

No authentication, authorization, or secret-handling changes. Metadata-only/frontend-only update.

## Migration and Rollout

- No backend migration required.
- Changes take effect on deploy.
- Browser/tab metadata may require hard refresh due to favicon/title caching behavior.

## Acceptance Criteria

- [x] Web app route titles use a consistent `Page | Oore CI` pattern
- [x] Web app base HTML includes updated title and social metadata
- [x] Docs site base HTML and VitePress config use `Oore CI Docs` titles
- [x] Manifests reflect `Oore CI` product naming
- [x] Validation gate (`make validate`) passes

## Owner

Platform team

## Last Updated

`2026-02-08`
