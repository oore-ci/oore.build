#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

print_live_acceptance() {
  local dependency
  for dependency in \
    'credentialed macOS runner' \
    'signing' \
    'source provider' \
    'object storage' \
    'email' \
    'external network' \
    'assistive technology' \
    'actual device'; do
    printf '[live acceptance] NOT RUN: %s\n' "$dependency"
  done
}

finish() {
  local status=$?
  if [[ "$status" -eq 0 ]]; then
    echo '[release-smoke] Release checks passed.'
  else
    echo '[release-smoke] Release checks failed.' >&2
  fi
  print_live_acceptance
}
trap finish EXIT

echo '[release-smoke] Checking release build inputs...'
make format-rust-check lint-rust check-openapi test-rust
