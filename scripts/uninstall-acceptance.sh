#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_DIR="$(mktemp -d)"
trap 'rm -rf "$TEST_DIR"' EXIT

export HOME="$TEST_DIR/home"
export XDG_CONFIG_HOME="$TEST_DIR/xdg"
mkdir -p "$HOME"

# Load uninstaller functions without running main.
UNINSTALLER_LIB="$TEST_DIR/uninstall-lib.sh"
sed '$d' "$ROOT_DIR/scripts/uninstall.sh" > "$UNINSTALLER_LIB"
# shellcheck disable=SC1090
source "$UNINSTALLER_LIB"

mkdir -p "$WEB_SYSTEMD_USER_DIR"
touch "$WEB_SYSTEMD_SERVICE_FILE"
SYSTEMCTL_LOG="$TEST_DIR/systemctl.log"
systemctl() { printf '%s\n' "$*" >> "$SYSTEMCTL_LOG"; }
uname() { printf 'Linux\n'; }

remove_local_web_systemd_user_service
remove_local_web_systemd_user_service

[[ ! -e "$WEB_SYSTEMD_SERVICE_FILE" ]]
grep -q -- '--user disable --now oore-web.service' "$SYSTEMCTL_LOG"
grep -q -- '--user daemon-reload' "$SYSTEMCTL_LOG"
grep -q -- '--user reset-failed oore-web.service' "$SYSTEMCTL_LOG"

echo "[uninstall-acceptance] passed"
