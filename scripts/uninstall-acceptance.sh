#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_DIR="$(mktemp -d)"
trap 'rm -rf "$TEST_DIR"' EXIT

cmp -s "$ROOT_DIR/scripts/uninstall.sh" "$ROOT_DIR/apps/site/public/uninstall"

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

assert_system_uninstall_uses_root_tools() (
  local source="$1"
  local name="$2"
  local case_dir="$TEST_DIR/$name"
  local uninstaller_lib="$case_dir/uninstall-lib.sh"
  local sudo_log="$case_dir/sudo.log"

  export HOME="$case_dir/home"
  export OORE_INSTALL_ROOT="$case_dir/install"
  mkdir -p "$HOME" "$OORE_INSTALL_ROOT/bin"
  sed '$d' "$source" > "$uninstaller_lib"
  # shellcheck disable=SC1090
  source "$uninstaller_lib"

  printf 'backend\n' > "$OORE_INSTALL_ROOT/INSTALL_MODE"
  export OORED_CALL="$case_dir/oored.log"
  printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "$OORED_CALL"\n' > "$BIN_DIR/oored"
  chmod +x "$BIN_DIR/oored"
  sudo() { printf '%s\n' "$*" >> "$sudo_log"; }
  launchctl() { return 0; }

  remove_daemon_launch_agent

  grep -q -- '^/bin/launchctl bootout system/build.oore.oored$' "$sudo_log"
  grep -q -- '^/bin/launchctl remove build.oore.oored$' "$sudo_log"
  grep -q -- '^/bin/rm -f /Library/LaunchDaemons/build.oore.oored.plist$' "$sudo_log"
  if grep -q -- "$BIN_DIR/oored" "$sudo_log"; then
    echo "[uninstall-acceptance] $name passed user-owned oored to sudo" >&2
    exit 1
  fi
  grep -q -- '^uninstall-service$' "$OORED_CALL"
)

assert_system_uninstall_uses_root_tools "$ROOT_DIR/scripts/uninstall.sh" scripts
assert_system_uninstall_uses_root_tools "$ROOT_DIR/apps/site/public/uninstall" public

echo "[uninstall-acceptance] passed"
