# Design System Governance and Frontend Compliance

## Status

`ready`

## Problem

The frontend codebase (`apps/web`) had inconsistent UI patterns: raw HTML elements (`<input>`, `<select>`, `<table>`), hand-coded inline SVGs, custom modal/drawer/dropdown implementations with manual event handling, and plain text loading states. Only 7 of 60+ shadcn components were used despite being configured. No design governance document existed to enforce consistency.

## User Impact

All frontend users see a more polished, consistent, and accessible UI. No functional changes — every user action (instance management, OIDC login, setup wizard, user management) works identically.

## UI Changes

- **Modals:** Custom `fixed inset-0` overlays replaced with shadcn `Dialog` and `AlertDialog`
- **Drawer:** Manual CSS-transform sidebar replaced with shadcn `Sheet`
- **Dropdowns:** Custom `useRef` + click-outside detection replaced with shadcn `DropdownMenu`
- **Tables:** Raw `<table>` replaced with shadcn `Table` components
- **Selects:** Raw `<select>` replaced with shadcn `Select` components
- **Inputs:** Raw `<input>` replaced with shadcn `Input`
- **Icons:** All inline SVGs replaced with Hugeicons via `@hugeicons/react`
- **Loading states:** Plain text replaced with `Spinner` component
- **Skeleton loading:** Users page shows skeleton layout while loading
- **Error states:** Raw text errors replaced with shadcn `Alert variant="destructive"`
- **Feedback:** Manual state-managed feedback banners replaced with Sonner toast
- **Status badges:** Hard-coded color classes replaced with shadcn `Badge` variants
- **Sidebar layout:** Header replaced with collapsible sidebar (shadcn `Sidebar`) with nav groups and user menu
- **Instance icon picker:** 24 curated Hugeicons selectable per instance; inline edit button on each instance row
- **Instance editing:** Edit dialog for existing instances (label, URL, icon)

## API Changes

None. This is a UI-only refactor.

## Security Considerations

No security impact. No changes to authentication, authorization, or data handling.

## Migration and Rollout

- No migration needed — this is a pure frontend refactor
- `DESIGN.md` added as mandatory governance document
- `AGENTS.md` and `CLAUDE.md` updated to reference `DESIGN.md`
- All future UI work must follow the design system rules

## Acceptance Criteria

- [x] `DESIGN.md` created with component selection rule, theming, icon, form, loading/error/feedback patterns
- [x] `AGENTS.md` updated with design system section
- [x] `CLAUDE.md` updated with design system reference
- [x] All inline SVG icons replaced with Hugeicons
- [x] Custom dialogs/modals use shadcn Dialog/AlertDialog
- [x] Custom drawer uses shadcn Sheet
- [x] Custom dropdown uses shadcn DropdownMenu
- [x] Raw HTML selects use shadcn Select
- [x] Raw HTML tables use shadcn Table
- [x] Loading states use Spinner/Skeleton
- [x] Error states use Alert
- [x] Feedback uses Sonner toast
- [x] `make build-web` passes
- [x] `make lint-web` passes
- [x] `make test-web` passes (55 tests)

## Owner

Platform team

## Last Updated

`2026-02-06`
