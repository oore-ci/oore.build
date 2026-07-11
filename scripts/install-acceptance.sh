#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Load installer functions without running main or reaching the network.
INSTALLER_LIB="$(mktemp)"
trap 'rm -f "$INSTALLER_LIB"' EXIT
sed '$d' "$ROOT_DIR/scripts/install.sh" > "$INSTALLER_LIB"
# shellcheck disable=SC1090
source "$INSTALLER_LIB"

OORE_ADVANCED=0
OORE_INSTALL_MODE=auto
RELEASE_OS=darwin
configure_install_mode
[[ "$OORE_INSTALL_MODE" == "all" ]]

OORE_DAEMON_LISTEN=""
OORE_DAEMON_URL="http://127.0.0.1:8787"
DAEMON_URL="$OORE_DAEMON_URL"
OORE_INSTALL_DAEMON_SERVICE=""
OORE_START_DAEMON=""
configure_backend_install
[[ "$OORE_DAEMON_LISTEN" == "127.0.0.1:8787" ]]
[[ "$OORE_INSTALL_DAEMON_SERVICE" == "true" ]]
[[ "$OORE_START_DAEMON" == "true" ]]

OORE_LOCAL_WEB_MODE=""
OORE_LOCAL_WEB_LISTEN=""
configure_frontend_install
[[ "$OORE_LOCAL_WEB_LISTEN" == "127.0.0.1:4173" ]]
[[ "$OORE_LOCAL_WEB_MODE" == "login" ]]

OORE_NONINTERACTIVE=1
OORE_OPEN_BROWSER=""
OORE_NO_OPEN=0
! should_open_browser
OORE_OPEN_BROWSER=true
should_open_browser
OORE_NO_OPEN=1
! should_open_browser

echo "[install-acceptance] passed"
