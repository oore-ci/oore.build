#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[release-smoke] Running local-first regression smoke checks..."

# Release installer local-first defaults and browser-open policy.
bash scripts/install-acceptance.sh

# Runner checkout reliability: nested submodules + explicit failure markers.
cargo test -p oore-runner checkout_

# Local-first onboarding critical path (loopback local login behavior).
cargo test -p oored --features test-support --test local_login_integration

# CLI flow coverage for login/config/status contract surfaces.
cargo test -p oore --test cli_unimplemented

echo "[release-smoke] All smoke checks passed."
