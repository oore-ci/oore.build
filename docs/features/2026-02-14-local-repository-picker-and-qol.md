# Local Repository Picker and QoL Improvements

## Status

`implemented`

## Problem

Adding a local repository required users to manually type absolute filesystem paths. This created high onboarding friction and made local-first setup feel error-prone.

## User Impact

- Users can now browse local folders from the UI instead of typing paths manually.
- Local repository setup is faster with quick location shortcuts and one-click path application.
- Display name is auto-suggested from the selected folder when left empty.

## UI Changes

- Redesigned `/settings/integrations/local-git` around an assisted add flow.
- Added `Browse` action with a folder picker dialog:
  - Current folder display
  - `Up` navigation
  - `Refresh`
  - Quick location buttons (Home, Desktop, Documents, Downloads, Code, Projects when present)
  - Directory list with `Git repo` badges
  - One-click `Use Repo` action for detected repositories
  - `Use Current Folder` when the current folder is already a git repository
- Added `Paste` action to fill repository path from clipboard.
- Replaced table/open-button repository list with clickable hoverable tiles; tile click opens source details and `Remove` remains a secondary action.
- Updated labels and copy to use repository-first language (`Add Repository`, `Remove`, etc.).
- Kept write actions permission-aware; read-only users get guidance instead of edit controls.

## API Changes

- Added `GET /v1/integrations/local-git/directories` (local mode only).
- New query parameter:
  - `path` (optional absolute path; defaults to daemon user home directory)
- New response schemas:
  - `BrowseLocalGitDirectoriesResponse`
  - `LocalGitDirectoryEntry`
  - `LocalGitPathSuggestion`
- Updated OpenAPI export and regenerated `apps/docs-site/docs/public/openapi.json`.

## Security Considerations

- Endpoint is authenticated and permission-gated with `integrations:write`.
- Endpoint is restricted to `local` runtime mode.
- Response returns directory metadata only (name/path/git marker), no file contents.
- Hidden directories are excluded from browse results to reduce accidental exposure/noise.

## Migration and Rollout

1. Deploy backend and frontend together.
2. No database migration required.
3. Existing local integrations continue to work unchanged.

## Acceptance Criteria

- [x] Local repository page supports click-based path selection.
- [x] Users can navigate folders and apply a detected git repo path without manual typing.
- [x] Clipboard paste and display-name auto-suggestion are available for faster input.
- [x] New browse endpoint is documented in OpenAPI.

## Owner

Platform team

## Last Updated

`2026-02-14`
