#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PUBLIC_DIR="$ROOT_DIR/apps/site/public"

mkdir -p "$PUBLIC_DIR"

sync_one() {
  local src="$1"
  local dst="$2"

  if [[ ! -f "$src" ]]; then
    echo "[sync-install] ERROR: source script not found: $src" >&2
    exit 1
  fi

  cp "$src" "$dst"
  chmod +x "$dst"
  echo "[sync-install] synced $(realpath "$dst" 2>/dev/null || echo "$dst")"
}

sync_one "$ROOT_DIR/scripts/install.sh" "$PUBLIC_DIR/install"
sync_one "$ROOT_DIR/scripts/uninstall.sh" "$PUBLIC_DIR/uninstall"

