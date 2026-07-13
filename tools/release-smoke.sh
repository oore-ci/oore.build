#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[release-smoke] Running local-first regression smoke checks..."

# Release automation must remain taggable and resilient to transient Pages outages.
grep -q '^  push:' .github/workflows/autotag.yml
grep -q '^  actions: write' .github/workflows/autotag.yml
grep -q '^  contents: write' .github/workflows/autotag.yml
grep -q 'gh workflow run release.yml' .github/workflows/autotag.yml
grep -q -- '--ref "${GITHUB_REF_NAME}"' .github/workflows/autotag.yml
grep -q '^  workflow_dispatch:' .github/workflows/release.yml
grep -q 'RELEASE_TAG:' .github/workflows/release.yml
grep -q 'uses: Swatinem/rust-cache@v2' .github/workflows/release.yml
grep -q 'cache-workspace-crates: true' .github/workflows/release.yml
grep -Fq 'key: ${{ matrix.target }}' .github/workflows/release.yml
if grep -q 'RELEASE_PAT' .github/workflows/autotag.yml; then
  echo "[release-smoke] Autotag must not depend on a personal access token." >&2
  exit 1
fi
grep -q 'deploy_pages deploy-site-only' .github/workflows/release.yml
grep -q 'deploy_pages deploy-docs-only' .github/workflows/release.yml
grep -q 'deploy_pages deploy-web-only' .github/workflows/release.yml
grep -q 'deploy_pages deploy-demo-only' .github/workflows/release.yml
if grep -Eq 'make deploy-(site|docs|web)-only &' .github/workflows/release.yml; then
  echo "[release-smoke] Pages deploys must not run concurrently." >&2
  exit 1
fi

# Release installer local-first defaults and browser-open policy.
bash scripts/install-acceptance.sh

# Runner checkout reliability: nested submodules + explicit failure markers.
cargo test -p oore-runner checkout_

# Local-first onboarding critical path (loopback local login behavior).
cargo test -p oored --features test-support --test local_login_integration

# Liveness and dependency-aware readiness stay distinct.
cargo test -p oored --features test-support --test setup_integration test_readyz_reports_runtime_dependencies

# CLI flow coverage for login/config/status contract surfaces.
cargo test -p oore --test cli_unimplemented

# Installed-state backup format and integrity verification.
cargo test -p oore --test cli_unimplemented backup_create_and_verify_round_trip
cargo test -p oore update_rollback_restores_previous_release

echo "[release-smoke] All smoke checks passed."
