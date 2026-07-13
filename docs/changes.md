# Change Ledger (Internal Docs Pointer)

This file is the only required in-repo internal documentation artifact.

Purpose:

- Provide a lightweight, reviewable ledger of behavior/contract changes.
- Point reviewers to the corresponding Linear doc(s) / ADR(s).

Rules:

- Any code change under `apps/`, `crates/`, `tools/`, etc. must add an entry here.
- Include a Linear issue/doc link for each entry.

## 2026-07-13

- **Repository source avatars**:
  - Repository sync now persists GitHub owner avatars and GitLab project avatars, falling back to a GitLab namespace avatar when a project has none. Project, source, and repository-picker views display that identity with a visible initials fallback for local or image-less repositories.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Compact demo-mode treatment**:
  - Demo builds now rely on the persistent compact Demo Mode indicator instead of repeating the same read-only notice in a full-width banner above every page.
  - The app header now reserves its navigation space for breadcrumbs, with duplicate Oore branding removed and a wider desktop search target.
  - Collapsed primary and admin sidebar navigation exposes the built-in item tooltips without replacing links with inert buttons.
  - Sidebar expansion is now a persisted UI preference, so refreshes and later browser sessions preserve the user's chosen working width.
  - The connectivity banner now reflects the browser's actual offline state instead of interpreting unrelated API errors or background-tab wakeups as proof that `oored` is unreachable.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Outcome-first build details and immersive logs**:
  - Build artifacts now stay above the fold beside the execution workspace on desktop and move into a peer tab beside Logs and Timeline on narrower screens; each file uses a compact direct-download action and one share-options action for copying temporary links or creating scoped links.
  - Build source, duration, and timing now sit in the title metadata instead of occupying a separate summary card.
  - Logs now use one internally scrollable GitHub-inspired workspace with a compact single-row toolbar, persistent search, pinned line numbers, comfortable vertical padding, complete-log defaults for successful builds, inline step navigation, and event history available as a neighboring tab. The workspace fills the remaining viewport with a deliberate bottom gutter on narrower screens and stays bounded on desktop.
  - Ordinary `stderr` output is no longer treated as failure severity. Only explicit error lines receive destructive styling or power the jump-to-error action, avoiding false alarms from Git progress and flags such as `--no-fatal-infos`.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Persistent runtime update notice**:
  - Instance owners now see available frontend and backend updates directly above the sidebar user menu instead of needing to discover them in Preferences.
  - The update dialog shows each runtime's current and target versions, managed-service readiness, generated release notes, and the GitHub comparison changelog before starting an update.
  - Runtime health and release checks now share one TanStack Query path across the sidebar and Preferences, with lightweight periodic refreshes so newly published updates appear without navigating away.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-runtime-updates-from-the-web-ui-6b648f19a3f9
- **Faster release builds**:
  - macOS release jobs now restore target-specific Cargo caches, including unchanged workspace crates, instead of compiling the complete Rust dependency graph twice for every release.
  - The daemon no longer pulls the unused AWS configuration, SSO, SSO-OIDC, and STS dependency chain merely to access the S3 behavior-version type.
  - Release operations documentation now reflects the PAT-free GitHub Actions flow and hosted-runner defaults.
  - Linear release channels doc: https://linear.app/oorebuild/document/release-channels-alpha-beta-stable-via-github-actions-993db297927a
- **Release dispatch branch propagation fix**:
  - Autotag now dispatches the Release workflow from GitHub's built-in branch ref instead of a step-local variable that was unavailable during dispatch, preventing alpha tags from accidentally starting the default-branch release definition.
  - Release smoke coverage locks the dispatch ref to `GITHUB_REF_NAME`.
  - Linear release channels doc: https://linear.app/oorebuild/document/release-channels-alpha-beta-stable-via-github-actions-993db297927a
- **Retention settings authorization fix**:
  - Global retention reads and writes now use the registered `instance_settings` RBAC resource, so owners and admins can load and manage retention instead of receiving a false `permission_denied` response.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-product-trust-hardening-release-592dfc525e77
- **PAT-free alpha release dispatch**:
  - Autotag now uses the repository-scoped GitHub Actions token to push release tags and explicitly dispatches the tag-aware Release workflow, removing the expired `RELEASE_PAT` bottleneck without losing downstream release execution to GitHub's recursion guard.
  - Release smoke coverage enforces the required token permissions, dispatch path, and absence of personal access-token coupling.
  - Linear release channels doc: https://linear.app/oorebuild/document/release-channels-alpha-beta-stable-via-github-actions-993db297927a
- **Graceful build-log transport fallback and explicit Android signing guidance**:
  - Build logs now treat SSE-to-polling fallback as a normal transport change instead of showing a persistent error while polling continues successfully.
  - Generated Android signing credentials use owner-only file permissions, and the signing guide now documents both zero-configuration temporary files and an explicit `CI=true` / `OORE_ANDROID_*` Gradle path.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5

- **Repository-first build setup**:
  - Empty projects now inspect the linked repository before asking users to configure a pipeline. Valid checked-in workflows are recommended with a secret-free preview; missing, invalid, loading, and provider-error states lead to an explicit next action instead of a dense blank form.
  - Repository execution fields stay read-only in the setup form so the checked-in file remains the single source of truth. Manual templates remain available as a deliberate fallback.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Reproducible manual build revisions**:
  - Manual and API builds that select a branch now resolve and store its exact commit before entering the queue. Config snapshots, reruns, and runner checkout retain that SHA even if the branch advances later.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Read-only repository workflow discovery API**:
  - Project maintainers can discover and semantically validate root `.oore.yaml`/`.oore.yml`, bounded `.oore/*.yaml|yml`, and an explicit repository-relative workflow path at a selected ref across GitHub, GitLab.com, self-managed GitLab, and local Git.
  - Discovery is protected by project `ManagePipelines` permission, limits file counts and response sizes, rejects unsafe paths and refs, and returns only secret-free execution previews: environment keys are visible but raw YAML, environment values, and provider credentials are never returned.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Repository workflow config path safety**:
  - Explicit repository workflow paths now use one workspace-relative path contract across pipeline create, update, dry-run validation, and runner execution. Absolute paths, traversal, dot or empty segments, backslash separators, oversized paths, and symlink escapes are rejected before the runner reads them.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Managed frontend onboarding and real Flutter migration fixes**:
  - A paired same-origin `oore-web` frontend now becomes the browser's instance automatically when no instance has been saved, so invited users can proceed directly to authentication; manual instance management remains available for generic and multi-instance clients.
  - Remote project creation now requires an explicitly selected connected repository and fills its editable name and default branch, while local project paths stay confined to Local Only mode.
  - Pipeline forms preserve platform arguments, environment variables, and artifacts independently from custom command toggles. Built-in artifact patterns and the runner now share the same valid workspace-relative glob contract.
  - Terminal build logs no longer present stale or duplicate live state, and the no-op client-only Validate action was removed in favor of validation on save.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Terminal log loading truth**:
  - A completed build now shows an explicit loading state while persisted logs are being fetched instead of briefly claiming the build recorded no logs.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5

- **Reliable release tagging and Pages deployment**:
  - Release tags are again cut from pushes to protected release-channel branches, avoiding a `workflow_run` trigger that GitHub only evaluates from the default branch.
  - Pages projects now deploy serially and retry only transient Cloudflare 5xx API failures with bounded backoff; authentication and configuration errors still fail immediately.
  - Linear release channels doc: https://linear.app/oorebuild/document/release-channels-alpha-beta-stable-via-woodpecker-github-releases-993db297927a
- **Runtime versions, owner-managed updates, and frontend state boundaries**:
  - Preferences now reports frontend and backend versions independently, checks the installed release channel for updates, and lets the owner update managed `oore-web` and `oored` services through their existing systemd/launchd supervisors.
  - Runner inventory now reports embedded and detached runner versions. Remote runner updates stay disabled until a runner-only package and managed service contract exist.
  - Shared command-palette state now lives in Zustand, server update state remains in TanStack Query, build-log lifecycle state uses a reducer, and pipeline-form state no longer mirrors React Hook Form dirtiness through an effect.
  - Linear feature docs: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5 and https://linear.app/oorebuild/document/feature-product-trust-hardening-release-592dfc525e77
- **Updater system-service authorization fix**:
  - `oore update` now obtains and explains macOS administrator authorization before replacing installed files, then restarts system launchd services non-interactively with bounded launchctl commands so a stalled password or service command cannot leave an apparently updated but unverified release.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-product-trust-hardening-release-592dfc525e77
- **Frontend release smoke follow-up**:
  - Signed-in AWS smoke testing passed at desktop and 390 px widths for dashboard, projects, builds, sources, and GitLab setup; long setup URLs now wrap within narrow hint cards.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Frontend layout and action consistency follow-up**:
  - Source connection actions now align across providers, connected-source and other inventory empty states use compact spacing, and equivalent navigation/create/copy/destructive actions share icons, wording, and visual treatment.
  - Shared page headers no longer double their section spacing; project cards align their actions; narrow settings layouts reflow instead of overflowing; and icon-only controls expose accessible names and selected state.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5

## 2026-07-12

- **Frontend build-log correctness foundation**:
  - Terminal build details now fetch the final log snapshot only after completion and merge it with streamed chunks by sequence, preventing visible output from regressing while persistence catches up.
  - Completed steps without runner log markers now default to All logs instead of selecting an empty step.
  - Build logs are now the primary full-width workspace, with a compact summary above and artifacts and event history moved into secondary sections below.
  - Step filtering now appears only for logs that can be associated truthfully; terminal controls use Base UI-backed shadcn components, semantic colors, and accessible labels.
  - Added focused regression coverage and removed render-phase routing/store mutations from setup, instance, project, and build flows.
  - Core build and project actions now distinguish navigation, source connection, settings, and build execution with consistent labels and icons.
  - Projects and builds now share the registry-backed Empty pattern, with consistent sentence-case actions and task-specific recovery guidance.
  - GitLab setup now guides admins through host choice, PAT/OAuth authentication, source verification, and webhook delivery readiness equally for GitLab.com and self-managed hosts.
  - GitLab webhook and OAuth callback instructions use the configured External Access public URL, preserving split AWS frontend and private macOS backend deployments.
  - GitLab setup schema, host choice, credentials, and webhook-secret controls now live in focused route-local modules while preserving the established setup flow.
  - Build detail routing, orchestration, summary, artifacts, and event history now live in cohesive single-component modules without changing route behavior.
  - Terminal log modeling, controls, step navigation, output, and ANSI rendering now live in cohesive modules behind the existing viewer API, with focused grouping and status-truth coverage.
  - Core routes and settings now use sentence-case action labels, consistent task-specific verbs, Button-owned icon sizing, and accessible names for icon-only user actions.
  - Verified Button, Select, Card, and Sidebar against the current shadcn registry and refreshed frontend runtime/tooling dependencies within their existing major versions; major upgrades remain isolated for later work.
  - Corrected vendor chunk boundaries so Base UI is no longer swallowed by the React matcher; route-only controls are deferred and initial JavaScript fell by 15.46 kB gzip (6.3%) after the dependency refresh.
  - Added a production entry-bundle gate (240 KiB JavaScript and 22 KiB CSS gzip) to `make validate`, measured from the assets actually referenced by the built HTML.
  - Dashboard build data now uses one query, build lists poll only while work is active, volatile build data keeps a five-second freshness window, and less volatile server state defaults to thirty seconds.
  - TanStack Query cancellation now reaches build, log, and artifact requests, including abort checks between paginated log pages.
  - Repository discovery now fetches independent source integrations concurrently and propagates cancellation through both integration and repository requests.
  - Log-stream polling now shares the stream lifecycle abort signal, preventing stale fallback requests from surviving build or instance changes.
  - React 19 Effect Events now keep global form and sidebar listeners current without mutating refs during render.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-frontend-product-quality-and-build-experience-overhaul-c257decee5c5
- **Updater runtime hotfix**:
  - `oore update` now runs its synchronous SQLite backup step on a blocking worker, avoiding a nested Tokio runtime panic after a release download has been verified.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-product-readiness-consistent-onboarding-and-first-class-gitlab-6e925460f155
- **Build-ready split runtime, consistent onboarding, and first-class GitLab**:
  - Private-address daemon installs now keep a loopback companion listener for the embedded runner without adding a wildcard/public bind, so backend readiness and runner readiness agree.
  - The dashboard and project/source flows now expose build-blocking runner state, preserve the required first-run action order, and avoid remote local-path dead ends.
  - Shared UI tokens and core onboarding/source screens now use consistent shape, color, heading, and action hierarchy.
  - GitLab.com and self-managed GitLab flows now provide split-proxy-safe OAuth/webhook guidance, hardened host validation, private checkout support, complete repository sync, and retry-safe webhook identity.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-product-readiness-consistent-onboarding-and-first-class-gitlab-6e925460f155
- **Verified launchd service installation**:
  - macOS service installation now retries modern `launchctl bootstrap`, requires `kickstart` and service lookup to succeed, and reports the real launchctl error instead of accepting the legacy `load` command's unreliable exit status.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-guided-split-deployment-installer-9da0d4bf02f6
- **Atomic installer executable upgrades**:
  - The installer now stages each executable beside its destination and atomically renames it into place, so reinstalling over a running macOS LaunchDaemon does not mutate its live executable inode and trigger `Killed: 9` during service replacement.
  - Installer acceptance coverage asserts that replacement changes the destination inode while preserving executable permissions and content.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-guided-split-deployment-installer-9da0d4bf02f6
- **Frontend Trusted Proxy pairing**:
  - `oore frontend invite` now creates a short-lived, single-use pairing code for a ready Trusted Proxy backend. A frontend-only installer can exchange `OORE_FRONTEND_PAIRING_CODE` through the private, CIDR-restricted `/v1/frontend/pair` capability, save the backend proof, and generate a separate local HAProxy-to-`oore-web` proof.
  - The split-role, installer, Mac Studio + NetBird + Warpgate, CLI, and OpenAPI documentation now describe pairing as the normal path while keeping manual proof files as an advanced fallback.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-guided-split-deployment-installer-9da0d4bf02f6
- **Headless macOS backend service**:
  - Backend-only installs now use a boot-time system LaunchDaemon running as the installing account, so SSH-only Mac build hosts do not require an active GUI login session.
  - Reinstall and uninstall remove any legacy user LaunchAgent left by an earlier backend installation attempt.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-guided-split-deployment-installer-9da0d4bf02f6
- **CI and release latency**:
  - Rust validation reuses dependency artifacts, release-branch path filtering compares only the current push, Pages deployment runs independently from binary packaging, and macOS ARM64/x86_64 release binaries build in parallel.
  - Release channels doc: https://linear.app/oorebuild/document/release-channels-alpha-beta-stable-via-woodpecker-github-releases-993db297927a
- **Installer explicit-role and timeout fix**:
  - Explicit macOS `backend` and `frontend` roles are no longer overwritten by the simple all-in-one default.
  - Release discovery and downloads now have bounded connection and transfer timeouts instead of hanging indefinitely on a broken network path.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-guided-split-deployment-installer-9da0d4bf02f6
- **Split deployment reliability**:
  - Frontend-only installs now reject occupied listen ports before changing service state and require both the auth-proxy proof and backend proof for Trusted Proxy identity forwarding.
  - `oore-web status` verifies launcher health plus dependency-aware backend readiness through the real frontend proxy path, while the proxy now forwards `/readyz` alongside `/healthz` and `/v1/*`.
  - `oore update` derives managed daemon and web addresses from launchd configuration, preserves custom private daemon addresses for unmanaged restarts, and verifies the real readiness endpoint before accepting an update or rollback.
  - Linux uninstall now disables, stops, removes, and reloads the `oore-web` systemd user service instead of leaving a broken lingering unit.
  - The Mac Studio + NetBird + Warpgate runbook now documents the deployed HAProxy topology, separate frontend/backend proofs, backend-owned owner initialization, and an unused loopback port for `oore-web`.
  - Linear feature docs: https://linear.app/oorebuild/document/feature-guided-split-deployment-installer-9da0d4bf02f6 and https://linear.app/oorebuild/document/feature-backend-owned-setup-init-for-local-and-trusted-proxy-modes-e850cb76e746
- Release automation now uses the configurable macOS runner with a `macos-latest` fallback for validation, autotagging, and release packaging, and bootstraps Bun plus both Rust macOS targets so an unavailable pre-provisioned self-hosted runner cannot block alpha delivery.
  - Release channels doc: https://linear.app/oorebuild/document/release-channels-alpha-beta-stable-via-woodpecker-github-releases-993db297927a
- **Product trust hardening release**:
  - Repository YAML now uses one strict parser across runner execution, daemon validation, and `oore pipeline validate`; repository YAML no longer accepts trigger/concurrency fields and artifact globs are safe workspace-relative patterns.
  - Runner protocol v2 prevents old runners from claiming work and adds artifact reservation, upload, completion/abort, pending visibility, stale cleanup, required-artifact failure, and `.app` bundle packaging.
  - Default macOS installation now installs and starts the loopback daemon and local web UI as launch-at-login services, opens the local web root for interactive installs, and relies on loopback local login instead of bootstrap tokens or `/setup` routing. Split and remote topology choices remain available through `scripts/install.sh --advanced`; `--no-open` and `OORE_OPEN_BROWSER` control browser opening.
  - `oore doctor` now separates core runner requirements from repeatable Android, iOS, and macOS platform checks. Java, Android SDK, and Xcode checks are target-specific; signing and notarization are warnings rather than release-runner blockers; JSON statuses are explicit.
  - Added liveness-only `/healthz` and dependency-aware `/readyz`; added verified `oore backup create|verify|restore`; and made `oore update` stage, back up, atomically replace, verify, and roll back installed releases while preserving managed service state.
  - Validation now covers master/alpha/beta/stable, release-tooling paths, and only runs autotag after Validate succeeds for the exact pushed commit.
  - Release smoke now exercises updater snapshot/install/rollback restoration, and build list/detail responses include project, pipeline, and runner display context for truthful operator UI.
  - Quick Debug APK pipelines now explicitly run `flutter build apk --debug`, and generated defaults use Flutter's real Android, iOS, and macOS output paths.
  - Pipeline creation preserves a successful create when a later signing request fails, then routes to a signing-only retry with the failed signing section expanded.
  - Builds show project context globally and accept optional named project/pipeline/runner context from the backend; terminal details prioritize failure reasons, failed steps, final-log states, and status-appropriate artifact empty states.
  - The hosted demo is explicitly sample-data, read-only UI: common build/project/pipeline mutations are visibly disabled and the API guard rejects all other mutations without returning fake success.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-product-trust-hardening-release-592dfc525e77

## 2026-05-22

- **Frontend guided setup hints**:
  - Builds empty state now spaces and aligns first-run actions consistently and explains the shortest path from source/project setup to the first build.
  - Source setup screens now surface GitHub App access, GitLab PAT scopes, webhook secret placement, built-in CI variables, and Android/iOS signing project hints directly in the UI.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

- **Same-origin proxy instance URL resolution**:
  - Web API hooks now resolve empty or `local` instance URLs to the current browser origin, so frontend-only/proxied deployments use the oore-web same-origin `/v1` proxy instead of silently disabling server-state queries.
  - Sources, setup status, login, project creation, and admin data hooks now share the same API base resolution for local-proxy instances.
  - Cleaned up the URL resolver to satisfy strict web linting in CI.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

- **Runtime version visibility and restart flag accuracy**:
  - Backend health and `oore-web` health responses now expose the loaded runtime version/channel so split deployments can confirm which frontend bundle and daemon are actually serving requests.
  - Instance preferences no longer hardcode `restart_required: true`; current supported preference changes apply without advertising a misleading pending restart.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

## 2026-05-21

- **Release latest resolution sorting**:
  - `oore-web update` and the installer now choose the highest semver-matching release for alpha/beta channels instead of trusting GitHub Releases list order.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

- **Remote auth mode preferences UI**:
  - Preferences now represents Remote OIDC and Remote Trusted Proxy as distinct access states instead of treating every Remote setup as OIDC.
  - Sources and source setup screens no longer default to Local Only while access policy is still loading, and their copy now follows the backend runtime mode instead of frontend-only assumptions.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

- **Web TypeScript release-build compatibility**:
  - Kept the required `ignoreDeprecations` compiler option and restored it to the TypeScript-supported `5.0` value so release builds pass `tsc`.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

- **Backend-owned setup initialization for trusted proxy deployments**:
  - Added `oore setup init --mode local|trusted-proxy` so backend-host setup can create the owner and complete setup without a browser bootstrap-token flow when the operator already knows the deployment mode.
  - Backend and installer trusted-proxy setup now require a shared secret for direct Trusted Proxy initialization, store it in restrictive files for service use, and let `oore-web` inject the backend secret only on proxied API requests.
  - `oore-web` now strips browser-supplied identity headers unless an upstream auth proxy proof header is present, preventing the frontend proxy from turning client headers into trusted backend identities.
  - Installer health checks no longer mark private-interface installs as failed just because the host cannot reach its own advertised address; they continue with a warning and clearer next steps.
  - Linear feature doc: https://linear.app/oorebuild/document/feature-backend-owned-setup-init-for-local-and-trusted-proxy-modes-e850cb76e746

- **Public setup/auth/install docs IA cleanup**:
  - Reframed public onboarding docs around backend-owned setup/auth with frontend clients kept dumb, and clarified the generic setup modes: Local Only, Remote OIDC, and Remote Trusted Proxy.
  - Updated install, first-instance, hosted UI, public alpha, setup-state, and feedback docs to distinguish install roles from setup modes, hosted UI from hosted backend, and split frontend/backend from provider-specific examples.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

- **Trusted-proxy sign-in UX**:
  - The web app now treats configured Remote + Trusted Proxy instances as proxy-authenticated sessions, auto-exchanging the forwarded identity header for an Oore session from both the dashboard guard and login page.
  - Trusted Proxy mode no longer falls through to Local Only or OIDC copy, and blocked Local Only access now points users back to daemon-host setup or their selected Remote auth mode instead of assuming OIDC.
  - Feature doc: https://linear.app/oorebuild/document/feature-guided-split-deployment-installer-9da0d4bf02f6

- **Frontend launcher update command**:
  - Added `oore-web update` for frontend-only hosts so operators can update the browser-facing launcher and `web-dist` assets from the installed release channel without rerunning the installer.
  - The command supports `--check`, `--force`, `--channel stable|beta|alpha`, and `--repo owner/name`, verifies the release checksum, and preserves installed `CHANNEL` / `GITHUB_REPO` metadata.
  - Feature doc: https://linear.app/oorebuild/document/feature-oore-web-frontend-update-command-6b648f19a3f9

## 2026-05-20

- **Daemon launchd service management**:
  - Implemented `oored install-service` and `oored uninstall-service` for macOS launchd user services.
  - `install-service` now writes `~/Library/LaunchAgents/build.oore.oored.plist`, starts the service by default, keeps the daemon alive, supports custom `--listen`, `--state-file`, `--label`, repeatable `--env KEY=VALUE`, and `--no-start`.
  - `uninstall-service` unloads/removes the service plist while leaving daemon data and logs untouched.
  - Install and uninstall scripts now use the same Oore CI banner/summary treatment in interactive and non-interactive runs, making CI logs and copied terminal output easier to follow.
  - Updated install, production deployment, clean reinstall, troubleshooting, and CLI docs so users can run persistent daemon setups without hand-writing launchd plists.
  - Feature doc: https://linear.app/oorebuild/document/feature-oored-launchd-service-installuninstall-3878b499a450

- **Guided split deployment installer**:
  - Installer modes are now role-based: `auto`, `all`, `backend`, and `frontend`; legacy `full` still works as an alias for `all`.
  - `backend` installs only the macOS daemon, CLI, and embedded runner from the backend archive; `all` keeps the prior single-host bundle with local web assets.
  - Frontend-only installs now prompt for a generic reachable backend URL, keep `oore-web` loopback by default, configure `run`/`login` service behavior, and can enable systemd lingering for Linux service survival across logout/reboot.
  - Non-interactive frontend-only installs now fail fast unless a backend URL was explicitly provided, preventing accidental proxies to `127.0.0.1`.
  - `all` and `backend` macOS installs now accept `OORE_DAEMON_LISTEN`, `OORE_PUBLIC_URL`, `OORE_CORS_ORIGINS`, and `OORE_INSTALL_DAEMON_SERVICE`, with interactive prompts for the same values.
  - Trusted-proxy setup now captures an initial owner email and rejects owner claims from a different forwarded identity, avoiding manual owner/admin edits in SQLite.
  - Trusted-proxy setup UI now includes proxy presets: generic `x-oore-user-email`, Warpgate `x-warpgate-username`, and a custom editable header.
  - Setup completion now promotes an existing matching user row to `owner` instead of ignoring it when the email already exists.
  - Installer prompts now start after the Oore CI banner/welcome, use consistent question-block formatting for text and option prompts, and backend installs only ask External Access/CORS questions when direct browser-to-daemon API access is selected or preconfigured.
  - Backend installs can now capture Trusted Proxy setup defaults (`OORE_SETUP_OWNER_EMAIL`, proxy preset, custom email header), pass them into the web setup wizard, and persist the selected daemon URL into `oore` CLI config so follow-up commands do not silently fall back to `127.0.0.1:8787`.
  - Split/backend installs no longer block the terminal waiting for browser setup completion.
  - Updated generic split role docs and kept the Mac Studio + NetBird + Warpgate guide as one provider-specific example.
  - Documented the prerelease installer endpoints so alpha/beta testers use the matching installer script, not the stable production installer.
  - Feature doc: https://linear.app/oorebuild/document/feature-guided-split-deployment-installer-9da0d4bf02f6

## 2026-05-16

- **Complexity optimization pass**:
  - Backend retention cleanup now bulk-loads candidate build artifacts per project instead of issuing one artifact query per candidate build.
  - Web log streaming now incrementally merges ordered log chunks instead of resorting the full log set on every SSE/poll append.
  - Project detail, log viewer, and user settings render paths now reuse memoized lookups/counts for repeated derived data.
  - Web TypeScript config keeps the supported deprecation suppression target for current `tsc` validation.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

## 2026-05-13

- **Frontend-only installer for split deployments**:
  - Added `OORE_INSTALL_MODE=frontend` to install only `oore-web` and prebuilt web assets on Linux or macOS hosts.
  - Release automation now publishes `oore-web_<version>_<os>_<arch>.tar.gz` frontend-only assets alongside macOS backend archives.
  - Installer supports `OORE_WEB_BACKEND_URL` so a separate frontend host can proxy `/v1/*` to a backend daemon over a private or controlled network path.
  - Docs now cover split backend/frontend topology, with Mac Studio + NetBird + Warpgate as one provider-specific example.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

## 2026-05-05

- **Project RBAC hardening from Codex scan** ([GitHub #88](https://github.com/devaryakjha/oore.build/issues/88), [#89](https://github.com/devaryakjha/oore.build/issues/89)):
  - `POST /v1/builds/{build_id}/cancel` now resolves the build project and requires `ProjectPermission::CancelBuild` before transitioning build state.
  - Scoped artifact token create/list/revoke routes now resolve artifact -> build -> project and require `ProjectPermission::ReadArtifacts` before minting or managing bearer download URLs.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

## 2026-04-15

- **Security fixes from Codex scan** ([GitHub #83](https://github.com/devaryakjha/oore.build/issues/83), [#84](https://github.com/devaryakjha/oore.build/issues/84), [#85](https://github.com/devaryakjha/oore.build/issues/85), [#86](https://github.com/devaryakjha/oore.build/issues/86)):
  - Enforced configured trusted-proxy shared secrets with the `X-Oore-Trusted-Proxy-Secret` header for runtime trusted-proxy login and setup owner claim.
  - Scoped direct build/log/artifact read routes to project RBAC, including stream-token issuance and artifact download links.
  - Scoped direct pipeline and signing mutation routes to project RBAC before applying pipeline, Android signing, iOS signing, device registration, or sync changes.
  - Rejected unsafe iOS signing filenames at API and runner materialization boundaries so signing assets cannot escape the runner workspace.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

- Deployment/auth docs now cover the internal-only Mac Studio rollout path behind NetBird + Warpgate instead of assuming remote setup is always OIDC-first.
  - Added an operations guide for serving the static web UI over internal HTTPS while keeping `oored` loopback-only, forwarding Warpgate identity headers, and completing setup in `Remote (Trusted Proxy / Warpgate)` mode.
  - Updated the deployment, hosted onboarding, first-instance, setup API, and auth API docs to document trusted-proxy setup/login and to correct loopback-vs-proxy guidance.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda

## 2026-03-19

- **Email notification channel (SMTP)** ([OOR-144](https://linear.app/oorebuild/issue/OOR-144)):
  - Added `email` as third notification channel type with SMTP provider config
  - New `encrypted_config` column for AES-256-GCM encrypted SMTP JSON blob
  - `lettre` crate for async SMTP with rustls TLS
  - HTML email templates for build status and runner-offline notifications
  - Frontend SMTP configuration form (create + partial edit)
  - `runner_offline` event filter exposed in UI
  - `SmtpConfig`, `UpdateSmtpConfig`, `SmtpTlsMode` contract types + OpenAPI
- **Artifact expiry and scoped download tokens** ([OOR-140](https://linear.app/oorebuild/issue/OOR-140/artifact-expiry-and-scoped-download-tokens)):
  - Added `expires_at` column to `artifacts` table; computed at creation from `artifact_ttl_days` retention policy setting.
  - Added `artifact_ttl_days` field to `RetentionPolicy`, `ProjectRetentionOverride`, and their update requests.
  - New `artifact_download_tokens` table for DB-backed scoped download tokens (survives daemon restart).
  - New backend module `artifact_tokens.rs` with 4 endpoints: `POST /v1/artifacts/{artifact_id}/scoped-token`, `GET /v1/artifacts/{artifact_id}/scoped-tokens`, `DELETE /v1/artifact-tokens/{token_id}`, `GET /v1/artifacts/dl/{token}`.
  - `GET /v1/artifacts/dl/{token}` is unauthenticated — the scoped token IS the authorization. Supports single-use and time-limited tokens.
  - Background `expired_artifact_monitor` task cleans up expired artifacts and download tokens every 5 minutes.
  - Frontend: artifact rows show expiry badge, new "Share Link" button opens dialog to create scoped tokens with configurable TTL and single-use option.
  - Migration `025_artifact_expiry_and_download_tokens.sql`.
  - OpenAPI spec updated with new endpoints and schemas.
- **API Tokens frontend** ([OOR-134](https://linear.app/oorebuild/issue/OOR-134)):
  - Added API token types (`CreateApiTokenRequest`, `CreateApiTokenResponse`, `ApiTokenSummary`, `ListApiTokensResponse`, `RevokeApiTokenResponse`) to `apps/web/src/lib/types.ts`.
  - Added `createApiToken`, `listApiTokens`, `revokeApiToken` API functions to `apps/web/src/lib/api.ts`.
  - Created `apps/web/src/hooks/use-api-tokens.ts` with `useApiTokens`, `useCreateApiToken`, `useRevokeApiToken` hooks.
  - Added `api_tokens:read/write/delete` permissions to RBAC matrix for owner, admin, and developer roles in `apps/web/src/hooks/use-permissions.ts`.
  - Created `apps/web/src/routes/settings/api-tokens.tsx` settings page with create dialog, token-revealed dialog, token table, and revoke confirmation.
  - Added "API Tokens" nav item to sidebar in `apps/web/src/components/nav-main.tsx`.
- **Fix: API token project-level role capping** ([OOR-134](https://linear.app/oorebuild/issue/OOR-134)):
  - `resolve_effective_project_role` now accepts `auth_source` and caps resolved project membership at the token's instance role when authenticated via API token. Prevents a downgraded token from inheriting the creator's full project permissions.
- **Build re-run / retry with same parameters** ([OOR-139](https://linear.app/oorebuild/issue/OOR-139/build-re-run-retry-with-same-parameters)):
  - Added `POST /v1/builds/{build_id}/rerun` endpoint that clones an existing build's `config_snapshot`, `branch`, `commit_sha`, and `pipeline_id` to enqueue a new build.
  - Added `source_build_id` column to `builds` table (migration 022) to link re-runs to their source build.
  - Added `RerunBuildResponse` contract type in `oore-contract`.
  - Frontend: replaced dialog-based re-run with single-click "Re-run" button on build detail page; added source build link for re-runs.
  - OpenAPI spec updated with new endpoint.
- **Runner health monitoring and status endpoint (OOR-142)**:
  - Backend: Added `GET /v1/runners/{runner_id}` endpoint for individual runner details.
  - Backend: Added `RunnerStateEvent` broadcast channel to emit events when runners go offline.
  - Backend: Notification dispatch now sends runner offline alerts to configured notification channels.
  - Backend: DB migration `022` extends `notification_deliveries` for runner event tracking (`runner_id`, `event_category`).
  - Frontend: Runner status dashboard now auto-refreshes every 15s via `refetchInterval`.
  - Frontend: Replaced "Rename policy" stat card with "Offline runners" count card with destructive badge.
  - Frontend: Added pulsing status dot indicators and stale heartbeat (>60s) warning highlighting.
  - OpenAPI spec updated with new `GET /v1/runners/{runner_id}` endpoint.
  - Linear: https://linear.app/oorebuild/issue/OOR-142/runner-health-monitoring-and-status-endpoint
- **SSO/OIDC provider management post-setup** ([OOR-141](https://linear.app/oorebuild/issue/OOR-141/ssooidc-provider-management-post-setup)):
  - Added `GET /v1/settings/external-access/oidc` endpoint to read current OIDC provider config (issuer, client ID, endpoints, configured_at). Never exposes client secret.
  - Added `POST /v1/settings/external-access/oidc/test-connection` endpoint for dry-run OIDC discovery validation without committing changes.
  - Fixed bug: `PUT /v1/settings/external-access/oidc` now clears pending auth entries on reconfigure, invalidating stale in-flight OIDC flows.
  - Frontend: OIDC identity card now displays current provider info (issuer, client ID, secret status).
  - Frontend: OIDC reconfigure dialog pre-populates from current config and includes "Test Connection" button.
  - Updated OpenAPI spec with new endpoints.

- **Documentation & CI maintenance fixes**:
  - CI: Reverted `actions/checkout@v4` back to `v6` in `validate.yml` for latest performance/security.
  - Docs: Updated clean-reinstall guide to provide robust macOS paths as primary instruction (no `jq` dependency).
  - Docs: Consolidated README screenshots in `apps/site/public/product/` to reflect renamed assets, while retaining local WebP assets in `apps/docs-site/docs/public` for documentation build reliability.
  - Docs: Added 'Auth-mode decision table' to the Public Alpha guide to clarify Local-only vs Remote (OIDC/Proxy) authentication requirements.
  - Tests: Added `clean-reinstall.md` and `issue-report-checklist.md` to documentation sanity test suite.
  - Hygiene: Added missing trailing newline to `.gitignore`.

## 2026-03-18

- **Doc improvements for early testers** ([#49](https://github.com/devaryakjha/oore.build/issues/49), [#44](https://github.com/devaryakjha/oore.build/issues/44), [#40](https://github.com/devaryakjha/oore.build/issues/40), [#48](https://github.com/devaryakjha/oore.build/issues/48), [#41](https://github.com/devaryakjha/oore.build/issues/41), [#42](https://github.com/devaryakjha/oore.build/issues/42), [#43](https://github.com/devaryakjha/oore.build/issues/43)):
  - Added "Alpha Feedback Playbook" with 10-minute test flow and templates.
  - Added "Issue Report Checklist" page and linked from SUPPORT.md.
  - Added screenshots with modern `.webp` formatting to the Public Alpha guide.
  - Added Cloudflared tunnel troubleshooting section to Public Alpha guide.
  - Added onboarding path decision table and release-channel reference table.
  - Linked playbook from README.md and SUPPORT.md.

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
  - Email channel implemented in [OOR-144](https://linear.app/oorebuild/issue/OOR-144).
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

## 2026-04-15

- Installer and bundled uninstall script UX follow-up: restored the local web onboarding path to the dedicated `/setup` route and preserved legacy `y`/`n` answers in the new numeric prompt selector so interactive shell flows keep working as before.
  - Docs index: https://linear.app/oorebuild/document/docs-index-linear-first-457d9edc9cda
