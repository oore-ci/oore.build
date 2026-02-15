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
