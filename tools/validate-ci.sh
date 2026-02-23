#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[validate-ci] Linting Woodpecker workflow config"
bash tools/lint-woodpecker.sh .woodpecker.yml

run_lane() {
  local lane_name="$1"
  shift

  echo "[validate-ci] lane=${lane_name} start"
  "$@"
  echo "[validate-ci] lane=${lane_name} done"
}

(
  run_lane "frontend-docs" make \
    docs-check \
    lint-web \
    lint-docs \
    test-web \
    test-docs \
    build-web \
    build-docs \
    build-site
) &
PID_FRONTEND=$!

(
  run_lane "rust" make \
    fmt-rust-check \
    test-rust-workspace \
    clippy-rust \
    cargo-check
) &
PID_RUST=$!

STATUS_FRONTEND=0
STATUS_RUST=0

wait "$PID_FRONTEND" || STATUS_FRONTEND=$?
wait "$PID_RUST" || STATUS_RUST=$?

if [[ "$STATUS_FRONTEND" -ne 0 || "$STATUS_RUST" -ne 0 ]]; then
  echo "[validate-ci] failed (frontend-docs=${STATUS_FRONTEND}, rust=${STATUS_RUST})" >&2
  exit 1
fi

echo "[validate-ci] all lanes passed"
