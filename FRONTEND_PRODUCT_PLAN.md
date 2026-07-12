# Frontend Product Quality Plan

Canonical execution checklist for the frontend product-quality overhaul. Product intent is mirrored in the [Linear feature doc](https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5); milestone status is maintained here with the implementation.

## Goal

Make Oore CI reliable and coherent for developers, QA teams, and occasional non-technical users without replacing the existing stack or visual identity.

## Baseline

- React Doctor: 38/100 (114 raw findings; confirmed findings are tracked below)
- Production entry payload: about 241 kB gzip JavaScript + 20 kB gzip CSS
- Largest initial chunk: `react-vendor`, about 170 kB gzip
- Frontend validation: 111 tests, lint, and production build passing

## Milestone 1 — Correctness and log truth

- [x] Fix conditional Hooks and render-time side effects in affected routes.
- [x] Fetch final logs only when a build becomes terminal and preserve streamed lines during the transition.
- [x] Default to All logs when step results cannot be associated with log markers.
- [x] Keep polling and SSE cleanup deterministic across build and instance changes.
- [x] Add focused regression coverage for terminal log transition and unattributed logs.

**Gate:** no Rules of Hooks errors; build completion cannot remove visible logs; focused tests, lint, and build pass.

## Milestone 2 — Build details and logs experience

- [x] Replace the overloaded metadata row with a compact build summary.
- [x] Make logs the primary full-width workspace; move artifacts and event history to secondary sections.
- [x] Show step navigation only when it provides truthful filtering.
- [x] Replace raw controls with shadcn/Base UI equivalents and label every icon-only action.
- [x] Make mobile logs usable with a persistent toolbar, deliberate wrapping/scrolling, and no overlays covering output.
- [x] Replace terminal hard-coded colors with semantic tokens supporting both themes.

**Gate:** build details work at desktop and 390 px widths, keyboard navigation is complete, and visual regression checks cover running, failed, successful, empty, and long-log states.

## Milestone 3 — Consistent product language

- [ ] Adopt one action grammar: create/add, connect/pair, navigate, retry, and destructive actions.
- [ ] Make Button own icon sizing instead of per-call numeric sizes.
- [ ] Standardize page headers, primary-action placement, empty states, loading states, and persistent errors.
- [ ] Update current shadcn components selectively, starting with Button, Select, Card, Sidebar, Empty, Field, and Button Group.
- [ ] Break up only the high-churn giant components: pipeline form, preferences, build detail, and terminal viewer.

**Gate:** core routes use the same action hierarchy and component patterns with no hard-coded application colors.

## Milestone 4 — GitLab product flow

- [ ] Separate host selection, authentication, connection verification, and webhook setup.
- [ ] Keep GitLab.com and self-managed GitLab equally supported.
- [ ] Explain PAT and OAuth trade-offs and minimum permissions without exposing secrets.
- [ ] Verify source connection and webhook readiness with clear recovery actions.

**Gate:** a first-time admin can connect a self-managed GitLab source without external instructions.

## Milestone 5 — Data and runtime performance

- [x] Consolidate duplicate dashboard build queries and poll only while active builds exist.
- [ ] Propagate TanStack Query abort signals through the API client.
- [ ] Run independent repository/source discovery requests concurrently.
- [x] Set query freshness by data volatility instead of one global five-second policy.
- [ ] Bound historical log memory/network use while preserving search and download behavior.

**Gate:** navigation cancels obsolete work, active builds update without focus changes, and source discovery does not scale sequentially with integration count.

## Milestone 6 — Bundle, packages, and release

- [x] Narrow the eager Base UI vendor chunk so route-only controls are not preloaded.
- [ ] Add a production bundle budget and a repeatable reporting command.
- [x] Verify core shadcn components against the current registry and update frontend dependencies within current major versions.
- [ ] Handle Vite, Vitest, ESLint, shadcn, Hugeicons, and TypeScript major upgrades separately.
- [ ] Run React Doctor regression scan, frontend checks, docs gate, and `make validate`.
- [ ] Publish an alpha release and complete signed-in desktop/mobile smoke testing.

**Gate:** initial payload is smaller than the baseline, validation is green, and the alpha build is testable through the real AWS frontend.

## Separate security track

Remote-mode HttpOnly cookie sessions require frontend/backend architecture work and are not part of the UI overhaul. Track and design this separately rather than mixing an authentication migration into visual and performance changes.
