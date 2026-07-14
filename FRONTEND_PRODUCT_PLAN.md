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

- [x] Adopt one action grammar: create/add, connect/pair, navigate, retry, and destructive actions.
- [x] Make Button own icon sizing instead of per-call numeric sizes.
- [ ] Standardize page headers, primary-action placement, empty states, loading states, and persistent errors.
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
