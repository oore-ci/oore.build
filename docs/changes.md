# Change Ledger (Internal Docs Pointer)

This file is the only required in-repo internal documentation artifact.

Purpose:
- Provide a lightweight, reviewable ledger of behavior/contract changes.
- Point reviewers to the corresponding Linear doc(s) / ADR(s).

Rules:
- Any code change under `apps/`, `crates/`, `tools/`, etc. must add an entry here.
- Include a Linear issue/doc link for each entry.

## 2026-03-18

- **CI migration: Woodpecker → GitHub Actions**:
  - Replaced `.woodpecker.yml` with 3 GitHub Actions workflows: `validate.yml`, `autotag.yml`, `release.yml`.
  - Validation split into parallel jobs: frontend/docs on Linux (`ubuntu-latest`), Rust on macOS (`macos-latest`). Saves ~70% of billable CI minutes.
  - Autotag runs on Linux (git+bash only, no macOS needed). Uses `RELEASE_PAT` secret to push tags that trigger the release workflow.
  - Release workflow: 4-job DAG — `build-assets` (macOS), `generate-notes` (Linux), `deploy-pages` (Linux), `github-release` (Linux). Uses `actions/upload-artifact` to pass build outputs between jobs.
  - Eliminated 50-line Python `gh` CLI download script — `gh` is pre-installed on GitHub Actions runners.
  - Caching: `Swatinem/rust-cache` for Cargo, `actions/cache` for `node_modules`.
  - Deleted: `.woodpecker.yml`, `tools/lint-woodpecker.sh`, `lint-woodpecker` Makefile target.
  - Updated: `tools/validate-ci.sh` (removed woodpecker lint call), `Makefile` (removed woodpecker references).

- **Ban direct useEffect — useMountEffect + ESLint enforcement** ([OOR-145](https://linear.app/oorebuild/issue/OOR-145)):
  - Frontend: Created `useMountEffect` hook as the only sanctioned wrapper for mount-only effects.
  - Frontend: Created sanctioned hooks for common reactive patterns: `useBreadcrumbLabel`, `useAutoScroll`, `useBuildNotification`, `useIndexAuthGuard`.
  - Frontend: Replaced all 58 direct `useEffect` calls across 35 files with proper patterns: derived state, react-hook-form `values` prop, `onOpenChange` callbacks, `useMountEffect`, event handlers, and named hooks.
  - Frontend: Added ESLint `no-restricted-syntax` rule banning `useEffect` / `React.useEffect` with exemptions for sanctioned hook files.
  - Frontend: Form resets now use react-hook-form `values` prop (runners, retention, notifications, project settings, setup/mode).
  - Frontend: Login page runtime mode now derived from `useSetupStatus()` query instead of one-shot fetch.

- **Demo mode audit: add missing MSW handlers** — [OOR-146](https://linear.app/oorebuild/issue/OOR-146):
  - Added demo handlers + fixture data for notification channels (CRUD, test, deliveries), build retention policy (global + per-project overrides), and audit log viewer (filtered, paginated).
  - All three features (OOR-143, OOR-137, OOR-135) now fully functional in `VITE_DEMO_MODE=true`.
  - New files: `demo/data/{notification-channels,retention,audit-logs}.ts`, `demo/handlers/{notifications,retention,audit-logs}.ts`.
  - Updated `demo/seed.ts` (added `NOTIFICATION_CHANNEL_IDS`) and `demo/handlers/index.ts`.

## 2026-03-17

- Audit log read endpoint and frontend viewer ([OOR-135](https://linear.app/oorebuild/issue/OOR-135)):
  - Backend: `GET /v1/audit-logs` with filtering (actor, action, resource type, date range) and pagination. RBAC: owner/admin only.
  - Frontend: `/settings/audit-log` page with table, filters, pagination. Added to sidebar nav under Admin.
  - Contract types: `AuditLogEntry`, `ListAuditLogsResponse` in `oore-contract`.
  - RBAC: `audit_logs:read` permission for owner and admin roles.
  - OpenAPI spec updated.

- **Notification channels (webhook + Mattermost)** — [OOR-143](https://linear.app/oorebuild/issue/OOR-143):
  - Backend: CRUD endpoints for notification channels under `/v1/settings/notification-channels` (create, list, get, update, delete, test, delivery history).
  - Backend: Background dispatch worker subscribes to `BuildStateEvent` broadcast and delivers notifications on terminal build states.
  - Backend: Fixed event publishing gap — `publish_event` now called after manual cancel and runner terminal transitions (not just timeout monitor).
  - Backend: Migration `019_notification_channels.sql` — `notification_channels` + `notification_deliveries` tables.
  - Backend: Webhook delivery with optional HMAC-SHA256 signing (`X-Oore-Signature`), Mattermost/Slack-compatible incoming webhook format.
  - Frontend: Settings UI for notification channels — list, create, edit, delete, test, delivery history.
  - Frontend: Sidebar nav entry under Admin section.
  - OpenAPI spec updated with Notification Channels tag and all new endpoints/schemas.
  - Email channel deferred to [OOR-144](https://linear.app/oorebuild/issue/OOR-144).
- **OOR-137: Build retention and cleanup policies** — automatic cleanup of old builds and artifacts.
  - Backend: retention policy engine with three criteria (max age, max count, max artifact size per project).
  - Global singleton settings table + per-project override table (migration 020).
  - Background cleanup job runs at configurable interval (default 1h), supports dry-run mode.
  - Two cleanup modes: `artifacts_only` (delete files, mark builds as Expired) or `full` (delete everything).
  - Storage deletion support added to `StorageBackend` (S3 + local).
  - `BuildStatus::valid_transitions()` updated: terminal states can now transition to `Expired`.
  - API endpoints: `GET/PUT /v1/settings/retention`, `GET /v1/settings/retention/last-cleanup`, `GET/PUT/DELETE /v1/projects/{project_id}/retention`.
  - Frontend: dedicated `/settings/retention` page with policy form, last-cleanup summary, protected statuses.
  - Navigation: "Retention" item added to admin sidebar.
  - Linear: https://linear.app/oorebuild/issue/OOR-137/build-retention-and-cleanup-policies

- **Granular RBAC with per-project permissions** (OOR-136, backend-only):
  - Added `project_members` table (migration 021) with per-project roles: `maintainer`, `developer`, `viewer`.
  - New `project_rbac` module: project-scoped authorization (`resolve_effective_project_role`, `check_project_permission`). Owner/admin bypass membership (implicit full access).
  - New API endpoints: `GET/POST /v1/projects/{id}/members`, `PATCH/DELETE /v1/projects/{id}/members/{user_id}`.
  - `list_projects` now filters by membership for non-admin users.
  - `get_project` returns `current_user_role` in response.
  - `update_project`, `delete_project`, `create_pipeline`, `list_pipelines`, `create_build` now use project-level permission checks.
  - Project creators are auto-added as `maintainer`.
  - No backfill: existing developer/qa_viewer users must be explicitly added to projects.
  - Contract types added to `oore-contract`: `ProjectRole`, `ProjectMember`, request/response types.
  - OpenAPI spec updated.
  - Linear: https://linear.app/oorebuild/issue/OOR-136/granular-rbac-with-per-project-permissions

- UX journey audit — multi-persona frontend fixes across `apps/web`:
  - Session expiry: added 5-minute warning toast + auto-redirect to login on expiry.
  - Build notifications: `document.title` updates with status emoji + browser Notification API on terminal state.
  - Build list: added status, branch, and project filter dropdowns + pagination integration.
  - Onboarding: getting-started checklist on empty dashboard, role-based welcome banner on first login.
  - Setup wizard: added mode descriptions, softened irreversible warning on complete step.
  - Invitations: auto-copies instance URL to clipboard on invite with share prompt.
  - Sidebar: added Documentation link (docs.oore.build).
  - Artifacts: added copy-link button for sharing download URLs.
  - Project detail: added "Latest successful build" shortcut on builds tab, permission-aware empty states.
  - Pipeline form: added contextual help text for signing/triggers, clearer manual-only trigger explanation.
  - Pipeline creation: added 5 template presets (Debug APK, Release Android, iOS+Android, All Platforms, Custom).
  - Log viewer: added ANSI SGR color support (16 colors, bold, dim, italic, underline).
  - Command palette: Cmd+K global search across projects, pages, and actions with keyboard navigation.
  - Header: added search trigger button (⌘K) for command palette discoverability.
  - Nav user: added role description in dropdown menu.
- UX journey audit — phase 2 (remaining frontend-fixable gaps):
  - Invite form: added role descriptions for all 4 roles, client-side email validation on blur.
  - Run Build button: tooltip explains why it's disabled (no pipelines / no source).
  - Dashboard recent builds: added Project column with name lookup.
  - Builds list: switched "Created" column from locale datetime to relative time for consistency.
  - Trigger build dialog: loading state for projects dropdown, branch/commit precedence help text.
  - Integration detail: humanized `auth_mode` enum values (e.g. `github_app_manifest` → "GitHub App (Manifest)").
  - Integration disconnect: button disabled while mutation is pending.
  - Artifacts panel: contextual empty state (terminal vs in-progress builds).
  - Artifact buttons: added `aria-label` with artifact name for accessibility.
  - Pipeline breadcrumb: dynamic name from loaded pipeline data (matches build detail pattern).
  - Clickable table rows: added `tabIndex`, `role="link"`, and keyboard handler (Enter/Space) across all list pages.

## 2026-03-16

- DX: added portless support for named `.localhost` dev URLs.
  - `apps/web` dev → `web.oore.localhost:1355`, `apps/docs-site` dev → `docs.oore.localhost:1355`, `apps/site` dev → `oore.localhost:1355`, `oored` daemon → `api.oore.localhost:1355`.
  - Added `make portless-proxy`, `make portless-alias-api`, `make portless-list` targets.
  - Vite proxy target now reads `OORED_URL` env var (falls back to `http://127.0.0.1:8787`).
  - Legacy `dev:legacy` scripts preserved for fallback without portless installed.

## 2026-02-25

- OOR-65 follow-up: fixed Pages post-deploy verification false negatives in tag releases when deployment metadata/environment shape differs from strict assumptions (for example short commit hashes and/or preview-vs-production listing differences).
  - `tools/verify-pages-deploy.sh` now supports robust commit metadata matching (`commit_hash`, `commitHash`, `commit_sha`, `sha`, `commit`) plus commit-message fallback, and uses environment fallback order in `auto` mode (`production -> preview -> all` for `stable`, `preview -> production -> all` otherwise).
  - Added `PAGES_VERIFY_ENVIRONMENT` control (`auto|production|preview|all`) and expanded summary output with matched environment/branch/commit for faster diagnosis.
  - Added bounded fresh-deployment fallback controls (`PAGES_VERIFY_FRESH_FALLBACK`, `PAGES_VERIFY_FRESH_WINDOW_SECONDS`) so verification still succeeds when Cloudflare deploy list omits branch/commit metadata; summaries now include `matched_by=metadata|fresh_fallback`.
  - Disabled the `tools/verify-pages-deploy.sh` hard gate in `.woodpecker.yml` tag release flow after repeated Cloudflare metadata mismatches; tag release success now depends on deploy command success only.
  - OOR-65: https://linear.app/oorebuild/issue/OOR-65

## 2026-02-23

- CI throughput hardening: consolidated duplicate `validate-pr`/`validate-push` into a single policy-equivalent `validate` step and switched CI execution to `make validate-ci` (parallel frontend/docs and rust lanes) while keeping `make validate` as the canonical local pre-handoff command.
  - OOR-65: https://linear.app/oorebuild/issue/OOR-65
- Added Woodpecker workflow lint gating with pinned `woodpecker-cli` (`v3.13.0` default) before CI validation lanes.
  - Feature doc: https://linear.app/oorebuild/document/feature-post-alpha-reliability-tranche-2026-02-22-db79675a84e3
- Reduced release pipeline critical path by moving tag release flow to an explicit DAG (`depends_on`), running release-note generation in parallel with build/deploy, and gating GitHub release publication on build + deploy + notes completion.
  - OOR-65: https://linear.app/oorebuild/issue/OOR-65
- Removed autotag script duplication in `.woodpecker.yml` by extracting shared semver/tag logic into `tools/autotag.sh` and retaining channel-specific entry steps only.
  - OOR-65: https://linear.app/oorebuild/issue/OOR-65
- Optimized Pages deploy/verify runtime while keeping strict correctness: deployed independent targets in parallel and rewrote `tools/verify-pages-deploy.sh` to support parallel polling with hard-fail semantics and tunable controls (`PAGES_VERIFY_MODE`, `PAGES_VERIFY_ATTEMPTS`, `PAGES_VERIFY_SLEEP_SECONDS`).
  - OOR-65: https://linear.app/oorebuild/issue/OOR-65

## 2026-02-15

- Migrated internal docs/ADRs/feature docs from repo `docs/` into Linear project “oore.build Docs”.
  - Linear index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda
- Hardened loopback-only endpoint enforcement by fixing forwarded-header IP parsing and applying it to External Access network updates.
  - OOR-6: https://linear.app/oorebuild/issue/OOR-6/harden-effective-client-ip-against-spoofed-loopback-via-forwarded
  - OOR-7: https://linear.app/oorebuild/issue/OOR-7/use-effective-client-ip-for-loopback-only-external-access-settings
- Clarified the auth contract: OIDC is required for non-loopback access (Remote mode), while Local Only mode supports loopback-only local login for onboarding (no passwords).
  - OOR-8: https://linear.app/oorebuild/issue/OOR-8/resolve-oidc-only-setup-gating-rule-conflict-with-local-loginauto
- Started migrating deployment + release automation from repo-local scripts to Woodpecker pipelines and GitHub Releases (tag-driven), and moved repo tooling from `scripts/` to `tools/`.
  - OOR-36: https://linear.app/oorebuild/issue/OOR-36/replace-scripts-makefile-releasedeploy-tooling-with-woodpecker-driven
- Fixed GitHub App install callback redirects to prefer the frontend origin (not the daemon `public_url`) when multiple External Access origins are configured.
  - OOR-34: https://linear.app/oorebuild/issue/OOR-34/github-installed-redirect-picks-wrong-origin-from-allowed-origins
- Ensured GitHub install-state cookie clearing derives `Secure` from the daemon origin (not the UI redirect), so cookies are reliably cleared when daemon/UI schemes differ.
  - OOR-37: https://linear.app/oorebuild/issue/OOR-37/github-installed-clears-install-state-cookie-with-wrong-secure-attr
- Moved local repository filesystem + git inspection work onto `spawn_blocking` to avoid blocking Tokio threads during project creation and local-git browsing/integration.
  - OOR-35: https://linear.app/oorebuild/issue/OOR-35/avoid-blocking-gitfs-operations-on-async-handlers-for-local-repos
- Dropped the `state.store` mutex before awaiting local repository inspection during project creation to avoid head-of-line blocking.
  - OOR-38: https://linear.app/oorebuild/issue/OOR-38/avoid-holding-store-mutex-across-local-repo-inspection-await-in-create
- Fixed Woodpecker configuration duplication (root `.woodpecker.yml` vs `.woodpecker/*.yml`) to prevent double pipeline runs, and aligned autotag to work on `master` as well as `main`.
  - OOR-36: https://linear.app/oorebuild/issue/OOR-36/replace-scripts-makefile-releasedeploy-tooling-with-woodpecker-driven
- Reduced duplicate validate runs by splitting validation into `pull_request` only (feature branches) and `push` only (default branches).
  - OOR-36: https://linear.app/oorebuild/issue/OOR-36/replace-scripts-makefile-releasedeploy-tooling-with-woodpecker-driven
- Added release channel support (alpha/beta/prod) via tag conventions and prerelease-aware GitHub Releases + environment-aware Pages deploy configuration.
  - OOR-36: https://linear.app/oorebuild/issue/OOR-36/replace-scripts-makefile-releasedeploy-tooling-with-woodpecker-driven
- Added branch-driven prerelease automation: merges to `alpha` / `release/alpha` cut `vX.Y.Z-alpha.N`; merges to `beta` / `release/beta` cut `vX.Y.Z-beta.N`; merges to `master`/`main` cut production tags.
  - OOR-36: https://linear.app/oorebuild/issue/OOR-36/replace-scripts-makefile-releasedeploy-tooling-with-woodpecker-driven
- Renamed release branches to `alpha`, `beta`, and `stable`; `master` remains a non-stable playground branch (no tagging).
  - OOR-36: https://linear.app/oorebuild/issue/OOR-36/replace-scripts-makefile-releasedeploy-tooling-with-woodpecker-driven

## 2026-02-16

- Added an opt-in Remote auth provider `trusted_proxy` (Warpgate/IAP) so first-time remote setup/login can complete without configuring Oore OIDC, while keeping per-user Oore sessions + RBAC + audit attribution.
  - ADR-0010: https://linear.app/oorebuild/document/adr-0010-remote-auth-providers-oidc-trusted-proxy-iap-cb4d4e4d52f5
  - Feature doc: https://linear.app/oorebuild/document/feature-remote-trusted-proxy-auth-mode-warpgate-8b6f6f698f75
- Added setup/runtime/settings APIs and preflight branching for `remote_auth_mode` (`oidc` vs `trusted_proxy`), including trusted-proxy peer/header validation and invite-only user mapping at login.
  - Feature doc: https://linear.app/oorebuild/document/feature-remote-trusted-proxy-auth-mode-warpgate-8b6f6f698f75
- Added a platform-contract amendment documenting Remote auth provider policy (`oidc` default + `trusted_proxy` opt-in) and trust-boundary constraints.
  - Contract amendment: https://linear.app/oorebuild/document/platform-contract-amendment-remote-auth-providers-9975f97813a8
- Fixed GitLab integrations to use provider-aware install sync, and relaxed OAuth host reachability precheck to treat auth-gated `/api/v4/version` (401/403) as reachable.
  - OOR-52: https://linear.app/oorebuild/issue/OOR-52/fix-gitlab-install-sync-dispatch-oauth-reachability-precheck
- Updated web demo mode (MSW handlers) to support latest onboarding + External Access APIs (`remote_auth_mode`, setup preferences, trusted proxy endpoints, and External Access settings/preflight routes).
  - OOR-53: https://linear.app/oorebuild/issue/OOR-53/update-web-demo-mode-for-remote-auth-external-access-endpoints
- Fixed docs OpenAPI pages to render the explorer client-only to avoid SSR `localStorage` errors during VitePress builds.
  - OOR-55: https://linear.app/oorebuild/issue/OOR-55/fix-docs-openapi-pages-ssr-errors-from-localstorage-access
- Hardened release-binary install/update flows: added channel-aware `latest` resolution (`OORE_CHANNEL`), persisted install metadata (`CHANNEL`, `GITHUB_REPO`), switched `oore update` to GitHub Releases (channel-aware), and made `oore version` / `oored version` report the installed `VERSION` (including `-alpha.N` / `-beta.N`).
  - OOR-36: https://linear.app/oorebuild/issue/OOR-36/replace-scripts-makefile-releasedeploy-tooling-with-woodpecker-driven
- Ensured Pages deploys are non-interactive (wrangler `--commit-dirty=true`) and added `oore-demo` Pages deployment to the tag release pipeline.
  - OOR-36: https://linear.app/oorebuild/issue/OOR-36/replace-scripts-makefile-releasedeploy-tooling-with-woodpecker-driven

## 2026-02-17

- Release readiness hardening: fixed lint/test gates so `make lint-*`, `make test-*`, and `cargo test --workspace` pass cleanly.
  - OOR-61: https://linear.app/oorebuild/issue/OOR-61/beta-readiness-fix-linttest-gates-webdocscli
  - Apps: added missing `eslint` dependency to `apps/docs-site`, ignored generated VitePress artifacts in lint, and made docs tests pass when no test files exist (`vitest --passWithNoTests`).
  - Web: resolved TypeScript lint failures (`no-unnecessary-condition`) and ignored a non-TS tool script in eslint config.
  - CLI: updated `oore` status CLI test to assert the current (implemented) behavior when the daemon is unreachable.
- Tooling: fixed `PAGES_BRANCH_FLAG` whitespace/empty-arg behavior so Pages deploy commands don’t fail when no branch is configured.
  - OOR-64: https://linear.app/oorebuild/issue/OOR-64/p0-fix-pages-branch-flag-adding-additional-space-to-wrangler-commands
- Installer: improved `OORE_CHANNEL=stable` behavior when no stable GitHub release exists yet (fallback + clearer guidance to use `OORE_CHANNEL=beta`/`alpha`).
  - OOR-61: https://linear.app/oorebuild/issue/OOR-61/beta-readiness-fix-linttest-gates-webdocscli
- Rust hardening: fixed all `cargo clippy --workspace --all-targets --all-features -- -D warnings` findings and aligned formatting (`cargo fmt`).
  - OOR-61: https://linear.app/oorebuild/issue/OOR-61/beta-readiness-fix-linttest-gates-webdocscli
- Tooling: expanded `make validate` to include lint, tests, `cargo fmt --check`, and strict clippy (`-D warnings`).
  - OOR-61: https://linear.app/oorebuild/issue/OOR-61/beta-readiness-fix-linttest-gates-webdocscli
- Docs: corrected production deployment prerequisites (OIDC default vs `trusted_proxy`) and removed a misleading landing-page claim about launchd service support.
  - OOR-61: https://linear.app/oorebuild/issue/OOR-61/beta-readiness-fix-linttest-gates-webdocscli
- Web: switched Zod imports to use the default export to avoid CI/runtime edge cases where the named `z` import is undefined.
  - OOR-61: https://linear.app/oorebuild/issue/OOR-61/beta-readiness-fix-linttest-gates-webdocscli
- Web demo mode: create the TanStack Router instance only after demo bootstrapping so deep links reliably seed storage and MSW intercepts page API requests.
  - OOR-63: https://linear.app/oorebuild/issue/OOR-63/p0-fix-requests-not-going-to-mock-service-worker-for-non-initial-pages
- Public alpha release docs: added a first-time onboarding “Public Alpha (v0.1.x)” page and updated docs homepage wording to reflect remote-vs-loopback auth reality.
  - OOR-62: https://linear.app/oorebuild/issue/OOR-62/public-alpha-release-messaging-onboarding-checklist-docs
- OOR-65: Fix Woodpecker CI duplication/inefficiency: avoid clone-only runs, avoid rebuilding web in release deploy, pin wrangler for reliable Pages deploys (ensure CI PATH includes Homebrew node; pass `--branch=stable` on prod tag deploys to avoid detached HEAD), and generate correct release compare links.
  - https://linear.app/oorebuild/issue/OOR-65/p0-build-pipelines-are-a-lot-flaky
- OOR-66: Fix branding across the repo: use “Oore CI” / “Oore” for user-facing product naming while keeping `oore.build` for domains and repo identifiers.
  - https://linear.app/oorebuild/issue/OOR-66/p0-fix-branding-across-the-repo
- Site: improved landing page contrast for secondary text and added a Demo link.
  - OOR-67: https://linear.app/oorebuild/issue/OOR-67/p0-improve-landing-page
- OOR-68: Site: redesign landing page (product-forward hero w/ demo UI screenshots) and add optional Cloudflare Web Analytics (page views only).
  - https://linear.app/oorebuild/issue/OOR-68/p0-improve-landing-page
  - Feature doc: https://linear.app/oorebuild/document/feature-landing-page-mission-brief-cloudflare-wa-c6f5c4bb91e4

## 2026-02-18

- Site performance follow-up for the landing page hero preview: added responsive WebP screenshot variants, prioritized LCP image discovery (`fetchpriority` + preload), and deferred Cloudflare analytics bootstrap until post-load idle time.
  - OOR-68: https://linear.app/oorebuild/issue/OOR-68/p0-improve-landing-page
  - Feature doc: https://linear.app/oorebuild/document/feature-landing-page-mission-brief-cloudflare-wa-c6f5c4bb91e4
- Community/repo launch readiness hardening: added `CODE_OF_CONDUCT.md`, `SUPPORT.md`, `.github/CODEOWNERS`, `.github/FUNDING.yml`, and new issue templates for feature requests, alpha test reports, and docs feedback.
  - OOR-69: https://linear.app/oorebuild/issue/OOR-69/initial-release-checklist-public-alpha
- Docs/launch readiness: added a public `Known Alpha Limitations (v0.1.x)` page and linked it from the Getting Started flow and README quick-start references.
  - OOR-69: https://linear.app/oorebuild/issue/OOR-69/initial-release-checklist-public-alpha
- Community onboarding follow-up: enabled GitHub Discussions, seeded 10 starter `good first issue` + `help wanted` tickets with acceptance criteria, and updated support/contact paths to route setup questions to Discussions.
  - OOR-69: https://linear.app/oorebuild/issue/OOR-69/initial-release-checklist-public-alpha
- Launch operations/release hygiene: added `.github/release.yml` changelog categories to surface breaking changes and migration notes, and moved the alpha launch-day runbook to an internal Linear document (not public docs site): https://linear.app/oorebuild/document/alpha-launch-day-runbook-internal-19638f05ed00
  - OOR-69: https://linear.app/oorebuild/issue/OOR-69/initial-release-checklist-public-alpha
- Installer prerelease-channel fix: corrected portable pattern matching in `scripts/install.sh` so `OORE_CHANNEL=beta` and `OORE_CHANNEL=alpha` resolve latest prerelease tags correctly on macOS shells (`sed`/`grep` portability).
  - OOR-69: https://linear.app/oorebuild/issue/OOR-69/initial-release-checklist-public-alpha

## 2026-02-22

- OOR-5 runner checkout reliability hardening: branch-based and commit-SHA checkout flows now guarantee recursive submodule sync/update (`git submodule sync --recursive` + `git submodule update --init --recursive`), and submodule failures are surfaced as explicit checkout-step hard failures.
  - OOR-5: https://linear.app/oorebuild/issue/OOR-5
- Added runner coverage for nested submodule checkout behavior and explicit failure-marker behavior using local fixture repositories.
  - OOR-5: https://linear.app/oorebuild/issue/OOR-5
- OOR-65 release reliability hardening: Pages deploy commands now pass commit metadata (`--commit-hash`, `--commit-message`), tag release pipeline now verifies all four Pages targets (`oore`, `oore-docs`, `oore-ci`, `oore-demo`) reached the expected branch/commit before success, and a deterministic local smoke gate (`make release-smoke`) was added.
  - OOR-65: https://linear.app/oorebuild/issue/OOR-65
  - Release channels doc: https://linear.app/oorebuild/document/release-channels-alpha-beta-stable-via-woodpecker-github-releases-993db297927a
- Expanded `oore` operator CLI alpha contract: implemented `oore login` (token import/validation + local-mode login), implemented `oore config set/get` with strict key whitelist (`daemon_url`, `session_token`) and exit code `2` for unsupported keys, expanded authenticated `oore status` summary, and expanded `oore doctor` with signing diagnostics plus JSON output.
  - V1 roadmap: https://linear.app/oorebuild/document/v1-implementation-roadmap-5e4fa12cdb04
- Updated user-facing docs for new CLI behavior, release verification flow, and alpha feedback intake (including a strict issue-report checklist page).
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda
- Added a consolidated Linear feature doc for this post-alpha reliability tranche.
  - Feature doc: https://linear.app/oorebuild/document/feature-post-alpha-reliability-tranche-2026-02-22-db79675a84e3
