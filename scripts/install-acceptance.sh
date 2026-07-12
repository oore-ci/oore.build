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

OORE_INSTALL_MODE=backend
configure_install_mode
[[ "$OORE_INSTALL_MODE" == "backend" ]]

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

service_call="$(mktemp)"
OORE_INSTALL_MODE=backend
OORE_DAEMON_LISTEN=100.64.0.10:8787
OORE_PUBLIC_URL=""
OORE_CORS_ORIGINS=""
DAEMON_URL=http://100.64.0.10:8787
DAEMON_LOG=/tmp/oored.log
BIN_DIR=/Users/appbuilder/.oore/bin
sudo() { printf '%s\n' "$*" > "$service_call"; }
id() { [[ "${1:-}" == "-un" ]] && printf 'appbuilder\n' || command id "$@"; }
curl_quick() { return 0; }
install_daemon_service
grep -q -- '/oored install-service --system --user appbuilder --listen 100.64.0.10:8787 --env HOME=' "$service_call"
unset -f sudo id curl_quick
rm -f "$service_call"

curl_args="$(mktemp)"
OORE_CHANNEL=alpha
OORE_VERSION=latest
OORE_RELEASES_LIST_URL=https://example.invalid/releases
TMP_DIR="$(mktemp -d)"
curl() {
  printf '%s\n' "$*" > "$curl_args"
  local previous=""
  for argument in "$@"; do
    if [[ "$previous" == "--output" ]]; then
      printf '[{"tag_name":"v1.0.0-alpha.1","draft":false,"prerelease":true}]\n' > "$argument"
      break
    fi
    previous="$argument"
  done
}
resolve_release_tag
[[ "$RELEASE_TAG" == "v1.0.0-alpha.1" ]]
grep -q -- '--connect-timeout 10 --max-time 60' "$curl_args"
unset -f curl
rm -rf "$TMP_DIR"
rm -f "$curl_args"

OORE_NONINTERACTIVE=1
OORE_OPEN_BROWSER=""
OORE_NO_OPEN=0
! should_open_browser
OORE_OPEN_BROWSER=true
should_open_browser
OORE_NO_OPEN=1
! should_open_browser

OORE_LOCAL_WEB_LISTEN="127.0.0.1:4173"
is_local_web_healthy() { return 1; }
lsof() { printf '4242\n'; }
if port_error="$(preflight_local_web_listen 2>&1)"; then
  echo "[install-acceptance] expected occupied listen preflight to fail" >&2
  exit 1
fi
[[ "$port_error" == *"127.0.0.1:4173 is already in use"* ]]

service_calls="$(mktemp)"
trap 'rm -f "$INSTALLER_LIB" "$service_calls"' EXIT
has_local_web_bundle() { return 0; }
systemctl() { printf '%s\n' "$*" >> "$service_calls"; }
if service_error="$(install_local_web_systemd_user_service 2>&1)"; then
  echo "[install-acceptance] expected occupied listen service install to fail" >&2
  exit 1
fi
[[ "$service_error" == *"127.0.0.1:4173 is already in use"* ]]
[[ ! -s "$service_calls" ]]

proof_dir="$(mktemp -d)"
trap 'rm -f "$INSTALLER_LIB" "$service_calls"; rm -rf "$proof_dir"' EXIT
printf 'backend-proof\n' > "$proof_dir/backend"
OORE_TRUSTED_PROXY_SHARED_SECRET=""
OORE_TRUSTED_PROXY_SHARED_SECRET_FILE="$proof_dir/backend"
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET=""
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE=""
if proof_error="$(ensure_frontend_secret_files 2>&1)"; then
  echo "[install-acceptance] expected one-proof frontend setup to fail" >&2
  exit 1
fi
[[ "$proof_error" == *"require a separate auth-proxy proof"* ]]

OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE="$proof_dir/backend"
if proof_error="$(ensure_frontend_secret_files 2>&1)"; then
  echo "[install-acceptance] expected same proof path to fail" >&2
  exit 1
fi
[[ "$proof_error" == *"must contain different values"* ]]

printf 'frontend-proof\n' > "$proof_dir/frontend"
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE="$proof_dir/frontend"
ensure_frontend_secret_files

printf 'backend-proof\n' > "$proof_dir/frontend"
if proof_error="$(ensure_frontend_secret_files 2>&1)"; then
  echo "[install-acceptance] expected matching proof values to fail" >&2
  exit 1
fi
[[ "$proof_error" == *"must contain different values"* ]]

printf ' \tbackend-proof \n\n' > "$proof_dir/frontend"
if proof_error="$(ensure_frontend_secret_files 2>&1)"; then
  echo "[install-acceptance] expected proofs equal after trimming to fail" >&2
  exit 1
fi
[[ "$proof_error" == *"must contain different values"* ]]

printf ' \t\n' > "$proof_dir/frontend"
if proof_error="$(ensure_frontend_secret_files 2>&1)"; then
  echo "[install-acceptance] expected whitespace-only proof to fail" >&2
  exit 1
fi
[[ "$proof_error" == *"empty after trimming whitespace"* ]]

printf 'frontend-proof\n' > "$proof_dir/frontend"
ensure_frontend_secret_files

bash "$ROOT_DIR/scripts/uninstall-acceptance.sh"

echo "[install-acceptance] passed"
