#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PUBLIC_DIR="$ROOT_DIR/apps/site/public"

mkdir -p "$PUBLIC_DIR"

# Sync install script
SOURCE_INSTALL="$ROOT_DIR/scripts/install.sh"
TARGET_INSTALL="$PUBLIC_DIR/install"

if [[ ! -f "$SOURCE_INSTALL" ]]; then
  echo "[sync-install] ERROR: source script not found: $SOURCE_INSTALL" >&2
  exit 1
fi

cp "$SOURCE_INSTALL" "$TARGET_INSTALL"
chmod +x "$TARGET_INSTALL"
echo "[sync-install] synced $TARGET_INSTALL"

# Sync uninstall script
SOURCE_UNINSTALL="$ROOT_DIR/scripts/uninstall.sh"
TARGET_UNINSTALL="$PUBLIC_DIR/uninstall"

if [[ ! -f "$SOURCE_UNINSTALL" ]]; then
  echo "[sync-install] ERROR: source script not found: $SOURCE_UNINSTALL" >&2
  exit 1
fi

cp "$SOURCE_UNINSTALL" "$TARGET_UNINSTALL"
chmod +x "$TARGET_UNINSTALL"
echo "[sync-install] synced $TARGET_UNINSTALL"
