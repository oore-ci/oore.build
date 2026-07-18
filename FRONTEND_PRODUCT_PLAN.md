# Frontend Product Quality Plan

Canonical execution checklist for the frontend product-quality overhaul. Product intent is mirrored in the [Linear feature doc](https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5); milestone status is maintained here with the implementation.

## Goal

Make Oore CI reliable and coherent for developers, QA teams, and occasional non-technical users without replacing the existing frontend stack. The approved visual direction is now a quiet technical system: shadcn Create's Neutral base, Amber as the default color theme, Vega as the default component style, the exact browser-local Create choices for both, Inter with JetBrains Mono for machine data, sentence-case copy, divider-led hierarchy, and minimal motion.

## Baseline

- React Doctor: 38/100 (114 raw findings; confirmed findings are tracked below)
- Production entry payload: about 241 kB gzip JavaScript + 20 kB gzip CSS
- Largest initial chunk: `react-vendor`, about 170 kB gzip
- Frontend validation: 111 tests, lint, and production build passing

## 2026-07-18 coherence pass — execution record

The approved whole-frontend redesign is implemented in the working tree. This is an execution record, not a new speculative design phase.

Completed decisions:

- Replaced the stale Google Sans/uppercase card-heavy contract with the quiet technical visual system documented in `DESIGN.md`.
- Standardized route structure on the real `PageLayout` and `PageHeader` primitives, compact sentence-case hierarchy, and cards only where they communicate an independent boundary.
- Standardized collection search, sorting, result context, pagination, compact phone rows, and destination-shaped loading/error/empty states using the shared collection controls instead of introducing a second generic table abstraction.
- Added the role-aware `/settings` hub and consolidated administrative destinations under one shared navigation model.
- Added a focused `/settings/theme` destination with all 18 Theme options from shadcn Create and the existing light, dark, and system modes. The browser-local choice is applied through the same injected `:root`/`.dark` variable-sheet method used by Create, including the initial loader, favicon, browser chrome, and a single shared app/sidebar primary accent.
- Added all eight shadcn Create component styles—Vega, Nova, Maia, Lyra, Mira, Luma, Sera, and Rhea—using the official neutral component hooks and managed `style-*` body-class mechanism. Style sheets load on demand, the choice persists on this browser, and rendered acceptance confirms each style changes real component geometry rather than only labels or colors.
- Made Direct runner enablement and repository approval understandable without treating approval of every repository as the desired or required state; large source inventories now use search, filters, and pagination, while sensitive GitLab webhook-token work is opt-in and project-scoped.
- Expanded the local demo into representative owner, admin, developer, and QA personas with small/large collections, runner-policy blocks, missing repositories, setup, degraded, empty, and active-build states so the product can be evaluated without production data.
- Reworked the VitePress documentation information architecture and OpenAPI loading path, and rebuilt the public site around current Direct runner, managed-update, and alpha-product truth.

Release evidence status:

- [x] Captured the final Owner/operating Dashboard and Builds views at 1440×900, generated exact 1200×750 and 720×450 WebP derivatives, refreshed the docs copies, and rebuilt the shared 1200×630 social image and raster mark assets.
- [x] Completed rendered Chromium acceptance at 1440×900 and 390×844 in both themes across Owner, Admin, Developer, and QA personas and the operating, blocked, degraded, empty, and setup demo scenarios. The app, docs, and public site showed no global overflow, broken images, or material hierarchy defects.
- [x] Passed the full non-Playwright validation contract: docs/lint/format gates, 267 web tests, demo/docs/site and release/update/performance tests, the Rust workspace, strict Clippy, every production bundle budget, all production builds, cargo check, frozen lockfile, generated OpenAPI parity, and diff checks.
- [ ] Complete the runtime Playwright browser matrix and physical iPhone Safari plus representative Android sweep. The current 69-test Playwright run stalled after preview startup, so it is explicitly not claimed green; the separate headless Chromium acceptance above is the bounded alternate evidence.
- [ ] Manually sync the canonical Linear Frontend Quality feature, Product Trust feature, Platform Contract, and V1 Roadmap; Linear was unavailable during this implementation pass.

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

- [x] Adopt one action grammar: create/add, connect/pair, navigate, retry, and destructive actions.
- [x] Make Button own icon sizing instead of per-call numeric sizes.
- [x] Standardize page headers, primary-action placement, empty states, loading states, and persistent errors.
- [x] Update installed shadcn components to the current v4 registry while preserving Oore-specific behavior and repeatable migration commands.
- [ ] Break up only the high-churn giant components: pipeline form, preferences, build detail, and terminal viewer.
  - [ ] Pipeline form
  - [ ] Preferences
  - [x] Build detail
  - [x] Terminal viewer

**Gate:** core routes use the same action hierarchy and component patterns with no hard-coded application colors.

## Milestone 4 — GitLab product flow

- [x] Separate host selection, authentication, connection verification, and webhook setup.
- [x] Keep GitLab.com and self-managed GitLab equally supported.
- [x] Explain PAT and OAuth trade-offs and minimum permissions without exposing secrets.
- [x] Verify source connection and webhook readiness with clear recovery actions.

**Gate:** a first-time admin can connect a self-managed GitLab source without external instructions.

## Milestone 5 — Data and runtime performance

- [x] Consolidate duplicate dashboard build queries and poll only while active builds exist.
- [ ] Propagate TanStack Query abort signals through the API client.
  - [x] Build, final-log, artifact, and live-log fallback requests
  - [x] Source integration and repository discovery requests
  - [ ] Remaining query-backed API reads
- [x] Run independent repository/source discovery requests concurrently.
- [x] Set query freshness by data volatility instead of one global five-second policy.
- [ ] Bound historical log memory/network use while preserving search and download behavior.

**Gate:** navigation cancels obsolete work, active builds update without focus changes, and source discovery does not scale sequentially with integration count.

## Milestone 6 — Bundle, packages, and release

- [x] Narrow the eager Base UI vendor chunk so route-only controls are not preloaded.
- [x] Add a production bundle budget and a repeatable reporting command.
- [x] Verify core shadcn components against the current registry and update frontend dependencies within current major versions.
- [x] Handle Vite, Vitest, Oxlint, shadcn, Hugeicons, and TypeScript major upgrades separately.
- [x] Run React Doctor regression scan, frontend checks, docs gate, and `make validate`.
- [x] Publish an alpha release and complete signed-in desktop/mobile smoke testing.

Release evidence: `v0.1.29-alpha.30` passed the release workflow and was verified through the signed-in AWS frontend at desktop and 390 px widths. Dashboard, projects, builds, sources, and GitLab setup loaded without page-level overflow. A live build-detail smoke remains dependent on creating the first real project and build; correctness and responsive log states are covered by the focused frontend suite in the meantime.

**Gate:** initial payload is smaller than the baseline, validation is green, and the alpha build is testable through the real AWS frontend.

## Separate security track

Remote-mode HttpOnly cookie sessions require frontend/backend architecture work and are not part of the UI overhaul. Track and design this separately rather than mixing an authentication migration into visual and performance changes.

## Milestone 7 — State boundaries and runtime administration

- [x] Keep server state in TanStack Query and form state in React Hook Form instead of duplicating either in Zustand.
- [x] Move cross-component command-palette state into the shared Zustand UI store.
- [x] Collapse coupled build-log stream flags into one reducer with deterministic cleanup.
- [x] Remove mirrored pipeline-form dirty state and group related disclosure state.
- [x] Show frontend and backend release versions independently.
- [x] Let owners update managed frontend and backend services from the web UI.
- [x] Report the installed version for embedded and detached runners.
- [ ] Add detached-runner remote updates after runner-only packaging and a managed runner service/supervisor contract exist.

**Gate:** shared, server, form, and lifecycle state each have one clear owner; runtime updates remain owner-only and unavailable for unmanaged processes; frontend/backend restart through their existing service managers.

## Milestone 8 — Real Flutter project migration

- [x] Make repository selection explicit; never create a project from an unshown fallback repository.
- [x] Use the connected repository's name and default branch as visible project defaults while keeping them editable.
- [x] Keep remote project creation focused on connected sources and local mode focused on local paths.
- [x] Auto-connect a paired same-origin frontend when a browser has no saved instances, while preserving manual multi-instance setup.
- [x] Keep UI-configured artifact paths consistent with the repository config and runner contracts.
- [x] Validate a mature FVM Flutter project flow using Kite's existing build/release commands as a read-only benchmark.
- [ ] Cover project creation, pipeline configuration, first build, logs, artifacts, and recovery UX with happy and failure paths.
- [ ] Publish an alpha release, then verify owner-driven frontend and backend updates from the real AWS UI.

**Gate:** a connected GitLab repository using `develop` can be turned into a truthful Android/iOS pipeline without editing or pushing to that repository; the first build either produces the configured artifacts or reports an actionable prerequisite failure; both managed Oore runtimes can be updated from the UI.

## Milestone 9 — Guided setup and repository-owned workflows

- [ ] Start project setup with an outcome-focused choice (for example, test an Android build or prepare a release) instead of presenting the full pipeline schema.
- [ ] Derive project name, default branch, Flutter/FVM metadata, available platforms, flavors, and likely artifacts from the selected repository where the source provider permits read access.
- [ ] Keep the common path short and progressively disclose triggers, command overrides, environment, artifacts, concurrency, and signing only when relevant.
- [x] Detect `.oore.yaml` and `.oore.yml` on the project's default branch before pipeline creation, including an explicitly configured path.
- [x] Preview every detected repository-owned workflow with its source path, resolved commands, platforms, artifacts, environment key names, and validation state before it is imported or run. Repository-owned triggers remain a later schema decision.
- [ ] Explain invalid config, unsupported keys, multiple-config conflicts, missing secrets, and runner/toolchain prerequisites with a concrete recovery action.
- [x] Keep repository config read-only unless a user explicitly asks Oore to prepare a change; never silently write or push workflow files.
- [ ] Test auto-detection, explicit paths, multiple workflows, config changes between commits, malformed YAML, missing config, and UI-fallback behavior across GitHub, GitLab.com, and self-managed GitLab.
- [ ] Use Kite read-only as the mature multi-workflow benchmark without copying its legacy pipeline defects or exposing repository secrets.

**Gate:** after selecting a repository, a new user can understand the recommended next action without CI-specific knowledge; when repository-owned Oore config exists, the UI visibly discovers and validates it before the first run, and the runner executes the exact config from the checked-out commit.

## Milestone 10 — Outcome-first build details and immersive logs

This follow-up supersedes Milestone 2's placement of artifacts as a secondary section. Real-project testing showed that a successful build's output is a primary outcome and must remain visible while its execution details are inspected.

- [x] Keep artifacts above the fold in a dedicated build-output column on desktop and before logs on narrow screens.
- [x] Make artifact download the visible primary row action and group copy/share utilities in a compact actions menu.
- [x] Present logs as one cohesive, full-height workspace with a persistent search field, line counts, download, wrapping, and explicit error navigation.
- [x] Keep step navigation and selected output in the same workspace: use a compact side rail on desktop and a horizontal step strip on narrow screens, default successful builds to the complete log, and focus running or failed builds on the relevant step.
- [x] Separate log transport from severity: ordinary `stderr` progress stays neutral and only explicit error lines receive destructive treatment.
- [x] Keep detailed event history one click away beside logs instead of requiring a page scroll.
- [x] Verify successful and failed step states in the local signed-in demo, including step filtering, full-log defaults, timeline switching, and false-positive severity cases.

**Gate:** the first viewport answers whether the build succeeded, what it produced, and where execution details live; successful Git and tool progress is visually calm; focused tests, production build, React Doctor, docs gate, and `make validate` pass.

## Milestone 11 — Stable UI consistency, responsive density, and role experiences

This milestone is a release-readiness audit and correction pass across the whole signed-in product. Repeated tasks must look and behave consistently enough for users to build muscle memory. A component may vary for role, device, or content only when that variation is deliberate and documented—not because separate screens evolved independently.

### P0 — release blockers and first impressions

- [ ] Audit every signed-in route in owner, developer, and QA contexts at 320, 390, 768, 1024, 1440, and 1920 px widths, in light and dark themes, with loading, empty, error, small-data, and large-data states where applicable.
- [ ] Verify the critical mobile flows on physical iPhone Safari, not only Chromium emulation.
- [x] Fix the Safari bottom safe-area/background failure by defining one token-based root surface for `html`, `body`, and the app mount; use `viewport-fit=cover`, safe-area padding, dynamic viewport units, and theme-aware browser chrome metadata.
- [x] Make the QA experience a first-class tester workspace rather than an empty or diminished operator dashboard: prioritize assigned apps/projects, latest installable build, active build state, and a compact recent history with useful empty and loading states.
- [x] Remove the obsolete “Explore as QA”/QA-preview feature end to end now that the demo environment can simulate roles. Remove its UI action, preview-session client state, child-instance behavior, backend endpoint, OpenAPI surface, and tests while preserving real QA accounts, permissions, and project-viewer enforcement.
- [x] Redesign Project access. Replace the current dense inline form-plus-table card with one clear member-management flow: a searchable user picker, contextual project-role choice, one obvious confirmation action, compact existing-member rows, and an overflow menu for secondary or destructive actions. Explain the QA viewer ceiling only when relevant. Use a shadcn dialog on wide screens and an appropriate compact mobile presentation; do not expose the desktop table on a narrow phone.
- [x] Fix the 768 px shell so navigation does not consume a desktop-width sidebar beside tablet-width content.
- [x] Remove divider and card nesting that does not communicate a real grouping, hierarchy, or interaction boundary.

### Shared collection and table contract

- [x] Inventory every collection screen—projects, builds, users, runners, sources, notifications, audit events, access lists, tokens, artifacts, and any embedded tables—and record its search, filter, sort, pagination, row-action, bulk-action, loading, empty, and error behavior before changing components.
- [x] Adopt one desktop table toolbar pattern: search at the leading edge, related filters immediately after it, bulk actions in a stable conditional slot, and table-specific secondary actions at the trailing edge. Keep the page's primary create/add action in the page header rather than moving it between table toolbars.
- [x] Add column sorting wherever the column is meaningful and the data source can return truthful results. Show the active direction, expose accessible labels, define a sensible default sort, and use server-side sorting for paginated data rather than sorting only the visible page.
- [x] Keep row actions in a rightmost actions column with the same overflow-menu treatment. Show a standalone row action only when it is the clear, frequent primary task for that row.
- [x] Use one table footer pattern: result/count context at the leading edge and pagination at the trailing edge. Keep page size, previous/next behavior, disabled states, and labels consistent.
- [x] Make search, filters, sorting, page, and page size URL-backed where restoration, sharing, or browser Back behavior is valuable.
- [x] At 640–1023 px, prioritize columns, hide secondary detail behind row expansion or the action menu, and wrap the toolbar in a fixed order. Do not make horizontal scrolling the default tablet solution.
- [x] Below 640 px, replace dense tables with compact list rows where comparison across columns is not essential. Put full-width search first, then consistently placed Filter and Sort controls; show identity, status, and the primary value before secondary metadata; keep actions predictable and touch-safe.
- [x] Reserve horizontal table scrolling on mobile for genuinely comparative datasets, keep the identity column visible where practical, and make the scroll affordance obvious.
- [x] Standardize row density, cell padding, header treatment, selected/hover/focus states, long-text truncation, status placement, timestamps, and destructive-action confirmation.
- [x] Match skeletons to the final rows, keep empty and error feedback inside the collection region, and avoid a page-level spinner for ordinary table refreshes.
- [x] Build shared toolbar, pagination, and responsive-row primitives only after at least two migrated screens prove the exact repeated shape; continue using shadcn Table and TanStack Table rather than creating a parallel table system.

### Page layout and interaction grammar

- [x] Standardize page title, description, breadcrumb, and primary-action placement through the existing page-header/layout primitives. On mobile, place the primary action in one predictable location and avoid oversized full-viewport headers.
- [x] Define compact, standard, and immersive page-width/padding modes and assign every route to one of them. Remove one-off container widths and breakpoint padding unless the content requires them.
- [x] Standardize button hierarchy, size, icon placement, disabled/loading state, and touch target. One action must not change visual rank or wording between equivalent screens.
- [x] Standardize search-field width and placement, filter-control order, overflow menus, destructive confirmations, form submit/cancel placement, and sticky actions for genuinely long forms.
- [x] Standardize tabs, cards, dialogs/sheets, alerts, skeletons, empty states, and persistent errors, including spacing and when a border or divider is warranted.
- [x] Review UX copy across repeated controls so identical actions use identical verbs and role-specific restrictions are explained at the point of action.
- [ ] Verify keyboard order, visible focus, screen-reader names, contrast, reduced motion, zoom at 200%, and minimum practical touch targets as part of each migrated pattern.

### Dashboard and mobile information density

- [x] Limit the home dashboard's Recent builds section to the six most recent completed builds and provide a clear “View all builds” route. Keep active/running builds separate so “recent” never becomes an unbounded build archive.
- [x] Reduce mobile card height, padding, decorative headers, and repeated metadata. Prefer compact rows or grouped summaries when a large card does not provide a distinct action or decision.
- [x] Keep the first mobile viewport focused on the user's next useful action and current system/build state instead of repeated chrome, oversized cards, or low-value history.
- [x] Review dashboard content by role so owners, developers, and QA users see the work and status relevant to them rather than the same widgets with most data missing.

### Performance and behavioral quality

- [x] Finish abort-signal propagation for remaining query-backed reads so navigation cannot leave obsolete work running.
- [ ] Bound historical log memory and network use without breaking search or full-log download.
- [x] Audit unnecessary duplicate queries, avoidable refetches, large rerender surfaces, long lists without windowing/pagination, layout shifts, and route chunks that load before their feature is opened.
- [ ] Measure representative dashboard, collection, project-access, and build-detail flows with small and large datasets; record interaction latency, query count, transferred bytes, and layout shifts before and after changes.
- [x] Keep loading transitions shaped like the destination UI and preserve prior table data during ordinary filter, sort, and page transitions where that prevents flicker without showing stale status as current.

### Audit evidence and rollout

- [ ] Maintain a route-by-role-by-viewport audit matrix with screenshot evidence and a severity-ranked issue ledger. Treat broken access, unreadable/hidden controls, Safari surface failures, and role dead ends as blockers; treat inconsistency, density, and polish as ranked follow-ups rather than losing them.
- [x] Establish the shared contracts on one representative collection screen and Project access, validate them with real data, then migrate the remaining screens in coherent batches.
- [x] Add visual and interaction regression coverage for the shared patterns at desktop, tablet, and mobile widths, including sort, filter, pagination, overflow actions, empty/error/loading states, and safe areas.
- [x] Run focused tests after each batch, then React Doctor, accessibility, visual, performance, docs, production builds, and `make validate` before release sign-off.
- [ ] Complete a final manual release sweep on Chrome, Firefox, and Safari desktop plus physical iPhone Safari and a representative Android browser.

**Gate:** equivalent screens teach one stable interaction model; tables and collection controls behave predictably at every target width; the dashboard is concise; Project access is understandable without instruction; QA users have a useful first-class workspace; obsolete QA preview code is gone; Safari paints the correct app surface through the bottom safe area; no release-blocking accessibility, responsive, or measured performance regression remains.
