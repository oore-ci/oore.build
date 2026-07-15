#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

update_bun_workspace() {
  local directory="$1"
  echo "[dependencies] Updating ${directory#$ROOT_DIR/}"
  (cd "$directory" && bun update --latest)
}

update_bun_workspace "$ROOT_DIR"
update_bun_workspace "$ROOT_DIR/apps/web"
update_bun_workspace "$ROOT_DIR/apps/docs-site"
update_bun_workspace "$ROOT_DIR/apps/site"

echo "[dependencies] Updating Rust lockfile within the installed Rust toolchain"
(
  cd "$ROOT_DIR"
  CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update
)

echo "[dependencies] Update complete. Run 'make ui-diff', then 'make validate'."
