# Change Ledger (Internal Docs Pointer)

This file is the only required in-repo internal documentation artifact.

Purpose:
- Provide a lightweight, reviewable ledger of behavior/contract changes.
- Point reviewers to the corresponding Linear doc(s) / ADR(s).

Rules:
- Any code change under `apps/`, `crates/`, `tools/`, etc. must add an entry here.
- Include a Linear issue/doc link for each entry.

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
