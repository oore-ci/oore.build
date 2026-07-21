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

atomic_dir="$(mktemp -d)"
printf 'old' > "$atomic_dir/oored"
printf 'new' > "$atomic_dir/source"
old_inode="$(ls -di "$atomic_dir/oored" | awk '{print $1}')"
install_executable "$atomic_dir/source" "$atomic_dir/oored"
new_inode="$(ls -di "$atomic_dir/oored" | awk '{print $1}')"
[[ "$new_inode" != "$old_inode" ]]
[[ -x "$atomic_dir/oored" ]]
[[ "$(< "$atomic_dir/oored")" == "new" ]]
rm -rf "$atomic_dir"

metadata_dir="$(mktemp -d)"
version_file="$metadata_dir/release-version"
printf '2.0.0\n' > "$version_file"
OORE_INSTALL_ROOT="$metadata_dir"
RESOLVED_CHANNEL=alpha
OORE_GITHUB_REPO=oore-ci/oore.build

printf '1.0.0\n' > "$metadata_dir/VERSION"
printf 'stable\n' > "$metadata_dir/CHANNEL"
printf 'backend/repository\n' > "$metadata_dir/GITHUB_REPO"
OORE_INSTALL_MODE=frontend
install_release_metadata "$version_file"
[[ "$(< "$metadata_dir/VERSION")" == "1.0.0" ]]
[[ "$(< "$metadata_dir/CHANNEL")" == "stable" ]]
[[ "$(< "$metadata_dir/GITHUB_REPO")" == "backend/repository" ]]
[[ "$(< "$metadata_dir/WEB_VERSION")" == "2.0.0" ]]
[[ "$(< "$metadata_dir/WEB_CHANNEL")" == "alpha" ]]
[[ "$(< "$metadata_dir/WEB_GITHUB_REPO")" == "oore-ci/oore.build" ]]

rm -rf "$metadata_dir"

transition_dir="$(mktemp -d)"
release_dir="$transition_dir/release"
TMP_DIR="$transition_dir/download"
OORE_INSTALL_ROOT="$transition_dir/install"
BIN_DIR="$OORE_INSTALL_ROOT/bin"
LOG_DIR="$OORE_INSTALL_ROOT/logs"
WEB_BINARY="$BIN_DIR/oore-web"
WEB_DIST_DIR="$OORE_INSTALL_ROOT/web-dist"
mkdir -p "$release_dir/bin" "$TMP_DIR" "$BIN_DIR" "$WEB_DIST_DIR"
printf 'new-oored\n' > "$release_dir/bin/oored"
printf 'new-oore\n' > "$release_dir/bin/oore"
chmod +x "$release_dir/bin/oored" "$release_dir/bin/oore"
printf '2.0.0\n' > "$release_dir/VERSION"
printf 'old-oore-web\n' > "$WEB_BINARY"
chmod +x "$WEB_BINARY"
printf 'old-web-dist\n' > "$WEB_DIST_DIR/index.html"
printf '1.5.0\n' > "$OORE_INSTALL_ROOT/WEB_VERSION"
printf 'beta\n' > "$OORE_INSTALL_ROOT/WEB_CHANNEL"
printf 'frontend/repository\n' > "$OORE_INSTALL_ROOT/WEB_GITHUB_REPO"
OORE_INSTALL_MODE=backend
RELEASE_VERSION=2.0.0
RELEASE_OS=darwin
RELEASE_ARCH=arm64
RESOLVED_CHANNEL=alpha
OORE_GITHUB_REPO=oore-ci/oore.build
tar -czf "$TMP_DIR/$(release_archive_name)" -C "$release_dir" .
install_binaries
[[ "$(< "$BIN_DIR/oored")" == "new-oored" ]]
[[ "$(< "$BIN_DIR/oore")" == "new-oore" ]]
[[ "$(< "$WEB_BINARY")" == "old-oore-web" ]]
[[ "$(< "$WEB_DIST_DIR/index.html")" == "old-web-dist" ]]
[[ "$(< "$OORE_INSTALL_ROOT/VERSION")" == "2.0.0" ]]
[[ "$(< "$OORE_INSTALL_ROOT/CHANNEL")" == "alpha" ]]
[[ "$(< "$OORE_INSTALL_ROOT/GITHUB_REPO")" == "oore-ci/oore.build" ]]
[[ "$(< "$OORE_INSTALL_ROOT/WEB_VERSION")" == "1.5.0" ]]
[[ "$(< "$OORE_INSTALL_ROOT/WEB_CHANNEL")" == "beta" ]]
[[ "$(< "$OORE_INSTALL_ROOT/WEB_GITHUB_REPO")" == "frontend/repository" ]]
[[ "$(< "$OORE_INSTALL_ROOT/INSTALL_MODE")" == "backend" ]]
rm -rf "$transition_dir"

managed_transition_dir="$(mktemp -d)"
managed_release_dir="$managed_transition_dir/release"
TMP_DIR="$managed_transition_dir/download"
OORE_INSTALL_ROOT="$managed_transition_dir/install"
BIN_DIR="$OORE_INSTALL_ROOT/bin"
LOG_DIR="$OORE_INSTALL_ROOT/logs"
WEB_BINARY="$BIN_DIR/oore-web"
WEB_DIST_DIR="$OORE_INSTALL_ROOT/web-dist"
DAEMON_LAUNCH_DAEMON_PLIST="$managed_transition_dir/build.oore.oored.plist"
DAEMON_LAUNCH_AGENT_PLIST="$managed_transition_dir/missing-launch-agent.plist"
candidate_call="$managed_transition_dir/candidate.log"
export CANDIDATE_CALL="$candidate_call"
mkdir -p "$managed_release_dir/bin" "$TMP_DIR" "$BIN_DIR" "$WEB_DIST_DIR"
printf '#!/bin/sh\nprintf "root=%%s\\n" "$OORE_INSTALL_ROOT" > "$CANDIDATE_CALL"\nprintf "args=%%s\\n" "$*" >> "$CANDIDATE_CALL"\n' > "$managed_release_dir/bin/oore"
printf 'new-oored\n' > "$managed_release_dir/bin/oored"
printf 'new-oore-web\n' > "$managed_release_dir/bin/oore-web"
printf 'new-web-dist\n' > "$managed_release_dir/web-index"
mkdir -p "$managed_release_dir/web-dist"
mv "$managed_release_dir/web-index" "$managed_release_dir/web-dist/index.html"
chmod +x "$managed_release_dir/bin/oore" "$managed_release_dir/bin/oored" "$managed_release_dir/bin/oore-web"
printf '2.1.0\n' > "$managed_release_dir/VERSION"
printf 'old-oore\n' > "$BIN_DIR/oore"
printf 'old-oored\n' > "$BIN_DIR/oored"
printf 'old-oore-web\n' > "$WEB_BINARY"
chmod +x "$BIN_DIR/oore" "$BIN_DIR/oored" "$WEB_BINARY"
printf 'old-web-dist\n' > "$WEB_DIST_DIR/index.html"
printf '1.9.0\n' > "$OORE_INSTALL_ROOT/VERSION"
printf 'all\n' > "$OORE_INSTALL_ROOT/INSTALL_MODE"
touch "$DAEMON_LAUNCH_DAEMON_PLIST"
OORE_INSTALL_MODE=all
RELEASE_VERSION=2.1.0
RELEASE_OS=darwin
RELEASE_ARCH=arm64
RESOLVED_CHANNEL=alpha
OORE_GITHUB_REPO=oore-ci/oore.build
MANAGED_BACKEND_UPGRADE=0
tar -czf "$TMP_DIR/$(release_archive_name)" -C "$managed_release_dir" .
install_binaries
[[ "$MANAGED_BACKEND_UPGRADE" -eq 1 ]]
[[ "$(< "$BIN_DIR/oore")" == "old-oore" ]]
[[ "$(< "$BIN_DIR/oored")" == "old-oored" ]]
[[ "$(< "$WEB_BINARY")" == "old-oore-web" ]]
[[ "$(< "$WEB_DIST_DIR/index.html")" == "old-web-dist" ]]
[[ "$(< "$OORE_INSTALL_ROOT/VERSION")" == "1.9.0" ]]
grep -q -- "^root=$OORE_INSTALL_ROOT$" "$candidate_call"
grep -q -- "^args=update --staged-release $TMP_DIR/extract --ensure-managed-runner --channel alpha --repo oore-ci/oore.build --force$" "$candidate_call"

rm -rf "$TMP_DIR/extract"
rm -f "$DAEMON_LAUNCH_DAEMON_PLIST"
DAEMON_LAUNCH_AGENT_PLIST="$managed_transition_dir/build.oore.oored.user.plist"
touch "$DAEMON_LAUNCH_AGENT_PLIST"
: > "$candidate_call"
MANAGED_BACKEND_UPGRADE=0
install_binaries
[[ "$MANAGED_BACKEND_UPGRADE" -eq 1 ]]
[[ "$(< "$BIN_DIR/oore")" == "old-oore" ]]
[[ "$(< "$BIN_DIR/oored")" == "old-oored" ]]
[[ "$(< "$OORE_INSTALL_ROOT/VERSION")" == "1.9.0" ]]
grep -q -- "^args=update --staged-release $TMP_DIR/extract --ensure-managed-runner --channel alpha --repo oore-ci/oore.build --force$" "$candidate_call"

rm -rf "$TMP_DIR/extract"
printf '#!/bin/sh\nexit 17\n' > "$managed_release_dir/bin/oore"
chmod +x "$managed_release_dir/bin/oore"
tar -czf "$TMP_DIR/$(release_archive_name)" -C "$managed_release_dir" .
MANAGED_BACKEND_UPGRADE=0
if install_binaries; then
  echo '[install-acceptance] failed staged upgrade unexpectedly fell through to shell installation' >&2
  exit 1
fi
[[ "$MANAGED_BACKEND_UPGRADE" -eq 0 ]]
[[ "$(< "$BIN_DIR/oore")" == "old-oore" ]]
[[ "$(< "$BIN_DIR/oored")" == "old-oored" ]]
[[ "$(< "$WEB_BINARY")" == "old-oore-web" ]]
[[ "$(< "$WEB_DIST_DIR/index.html")" == "old-web-dist" ]]
[[ "$(< "$OORE_INSTALL_ROOT/VERSION")" == "1.9.0" ]]
rm -rf "$managed_transition_dir"

sudo_call="$(mktemp)"
oored_call="$(mktemp)"
oore_call="$(mktemp)"
service_bin_dir="$(mktemp -d)"
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "$OORED_CALL"\n' > "$service_bin_dir/oored"
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "$OORE_CALL"\n' > "$service_bin_dir/oore"
chmod +x "$service_bin_dir/oored" "$service_bin_dir/oore"
export OORED_CALL="$oored_call"
export OORE_CALL="$oore_call"
OORE_INSTALL_MODE=all
OORE_DAEMON_LISTEN=100.64.0.10:8787
OORE_PUBLIC_URL='https://ci.example.test/?a=1&b=2'
OORE_WARPGATE_TICKET='opaque-ticket'
OORE_ARTIFACT_DELIVERY_URL='https://artifacts.example.test'
OORE_CORS_ORIGINS='https://app.example.test'
DAEMON_URL=http://100.64.0.10:8787
BIN_DIR="$service_bin_dir"
OORE_INSTALL_ROOT="$service_bin_dir"
LOG_DIR="$service_bin_dir/logs"
DAEMON_LOG="$LOG_DIR/oored.log"
DAEMON_LAUNCH_DAEMON_PLIST="$service_bin_dir/build.oore.oored.plist"
sudo() {
  printf '%s\n' "$*" >> "$sudo_call"
  case "$1" in
    /usr/bin/install)
      local destination=""
      for destination in "${@:2}"; do :; done
      : > "$destination"
      command /bin/chmod 0600 "$destination"
      ;;
    /usr/bin/tee) command /usr/bin/tee "${@:2}" ;;
    /bin/chmod) command /bin/chmod "${@:2}" ;;
    /bin/mv) command /bin/mv "${@:2}" ;;
    /bin/rm) command /bin/rm "${@:2}" ;;
    /usr/bin/plutil|/bin/launchctl) return 0 ;;
    /usr/bin/stat) printf 'root:wheel:600\n' ;;
    /usr/libexec/PlistBuddy)
      case "$3" in
        'Print :UserName') printf 'appbuilder\n' ;;
        'Print :ProgramArguments:0') printf '%s/oored\n' "$service_bin_dir" ;;
        *) return 1 ;;
      esac
      ;;
    *) return 1 ;;
  esac
}
id() { [[ "${1:-}" == "-un" ]] && printf 'appbuilder\n' || command id "$@"; }
curl_quick() { return 0; }
if command -v plutil >/dev/null 2>&1; then
  render_system_daemon_plist appbuilder | plutil -lint - >/dev/null
fi
install_daemon_service
install_runner_service
grep -q -- '^uninstall-service$' "$oored_call"
grep -q -- '^runner install-service --managed-local --daemon-url http://127.0.0.1:8787$' "$oore_call"
OORE_DAEMON_LISTEN='[fd00::10]:9797'
[[ "$(runner_loopback_url)" == 'http://[::1]:9797' ]]
OORE_DAEMON_LISTEN='127.0.0.2:9898'
[[ "$(runner_loopback_url)" == 'http://127.0.0.2:9898' ]]
OORE_DAEMON_LISTEN=100.64.0.10:8787
grep -q -- '^/usr/bin/install -o root -g wheel -m 0600 /dev/null .*build.oore.oored.plist.install.' "$sudo_call"
grep -q -- '^/usr/bin/tee .*build.oore.oored.plist.install.' "$sudo_call"
grep -q -- '^/bin/launchctl bootstrap system .*build.oore.oored.plist$' "$sudo_call"
grep -q -- '^/bin/launchctl kickstart -k system/build.oore.oored$' "$sudo_call"
if grep -q -- "$service_bin_dir/oored install-service\|$service_bin_dir/oored uninstall-service" "$sudo_call"; then
  echo '[install-acceptance] user-owned oored crossed the sudo boundary' >&2
  exit 1
fi
while read -r privileged_program _; do
  case "$privileged_program" in
    /bin/chmod|/bin/launchctl|/bin/mv|/bin/rm|/usr/bin/install|/usr/bin/plutil|/usr/bin/stat|/usr/bin/tee|/usr/libexec/PlistBuddy) ;;
    *)
      echo "[install-acceptance] unexpected privileged program: $privileged_program" >&2
      exit 1
      ;;
  esac
done < "$sudo_call"
[[ "$(stat -f '%Lp' "$DAEMON_LAUNCH_DAEMON_PLIST" 2>/dev/null || stat -c '%a' "$DAEMON_LAUNCH_DAEMON_PLIST")" == "600" ]]
grep -q -- '<key>UserName</key>' "$DAEMON_LAUNCH_DAEMON_PLIST"
grep -q -- '<string>appbuilder</string>' "$DAEMON_LAUNCH_DAEMON_PLIST"
grep -q -- "<string>$service_bin_dir/oored</string>" "$DAEMON_LAUNCH_DAEMON_PLIST"
grep -q -- '<string>https://ci.example.test/?a=1&amp;b=2</string>' "$DAEMON_LAUNCH_DAEMON_PLIST"
grep -q -- '<key>OORE_WARPGATE_TICKET</key>' "$DAEMON_LAUNCH_DAEMON_PLIST"
grep -q -- '<string>opaque-ticket</string>' "$DAEMON_LAUNCH_DAEMON_PLIST"
unset -f sudo id curl_quick
rm -rf "$service_bin_dir"
rm -f "$sudo_call" "$oored_call" "$oore_call"

curl_args="$(mktemp)"
OORE_CHANNEL=alpha
OORE_VERSION=latest
OORE_RELEASE_MANIFEST_URL=https://example.invalid/latest/alpha.json
TMP_DIR="$(mktemp -d)"
curl() {
  printf '%s\n' "$*" > "$curl_args"
  local previous=""
  for argument in "$@"; do
    if [[ "$previous" == "--output" ]]; then
      printf '{"schema_version":1,"channel":"alpha","tag":"v1.0.0-alpha.1"}\n' > "$argument"
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

LOCAL_WEB_URL="http://127.0.0.1:4173"
curl_quick() { printf '<html>not oore-web</html>\n'; }
if is_local_web_healthy; then
  echo '[install-acceptance] HTML must not satisfy oore-web health' >&2
  exit 1
fi
curl_quick() { printf '{"ok":true,"version":"test"}\n'; }
is_local_web_healthy
unset -f curl_quick

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

OORE_WEB_BACKEND_URL="http://100.107.193.1:8787"
OORE_LOCAL_WEB_LISTEN="127.0.0.1:4174"
OORE_WEB_BACKEND_TRANSPORT_PROTECTED=false
if transport_error="$(validate_web_transport_config 2>&1)"; then
  echo '[install-acceptance] expected unprotected remote HTTP backend to fail' >&2
  exit 1
fi
[[ "$transport_error" == *"OORE_WEB_BACKEND_TRANSPORT_PROTECTED=true"* ]]

for unsafe_backend_url in \
  'HTTP://100.107.193.1:8787' \
  'http://localhost:8787@100.107.193.1:8787'; do
  OORE_WEB_BACKEND_URL="$unsafe_backend_url"
  if transport_error="$(validate_web_transport_config 2>&1)"; then
    echo "[install-acceptance] unsafe backend URL passed validation: $unsafe_backend_url" >&2
    exit 1
  fi
done
OORE_WEB_BACKEND_URL="http://100.107.193.1:8787"

(
  OORE_WEB_BACKEND_TRANSPORT_PROTECTED=false
  OORE_WEB_BROWSER_TRANSPORT_PROTECTED=false
  OORE_LOCAL_WEB_LISTEN="100.107.193.2:4174"
  is_noninteractive() { return 1; }
  has_prompt_tty() { return 0; }
  prompt_select() { printf 'yes\n'; }
  configure_web_transport_assertions
  [[ "$OORE_WEB_BACKEND_TRANSPORT_PROTECTED" == "true" ]]
  [[ "$OORE_WEB_BROWSER_TRANSPORT_PROTECTED" == "true" ]]
)

service_root="$(mktemp -d)"
OORE_WEB_BACKEND_TRANSPORT_PROTECTED=true
OORE_WEB_BROWSER_TRANSPORT_PROTECTED=false
WEB_BACKEND_URL="$OORE_WEB_BACKEND_URL"
WEB_BINARY="/home/ubuntu/.oore/bin/oore-web"
WEB_DIST_DIR="/home/ubuntu/.oore/web-dist"
WEB_SYSTEMD_USER_DIR="$service_root"
WEB_SYSTEMD_SERVICE_FILE="$service_root/oore-web.service"
LOG_DIR="$service_root/logs"
lsof() { return 0; }
enable_linux_lingering() { return 0; }
install_local_web_systemd_user_service
grep -q -- '--backend-transport-protected' "$WEB_SYSTEMD_SERVICE_FILE"
if grep -q -- '--browser-transport-protected' "$WEB_SYSTEMD_SERVICE_FILE"; then
  echo '[install-acceptance] disabled browser transport assertion leaked into service' >&2
  exit 1
fi
launchd_args="$(web_transport_launchd_args)"
[[ "$launchd_args" == *'<string>--backend-transport-protected</string>'* ]]
rm -rf "$service_root"

proof_dir="$(mktemp -d)"
trap 'rm -f "$INSTALLER_LIB" "$service_calls"; rm -rf "$proof_dir"' EXIT

for mode in 600 640 644 666; do
  proof_path="$proof_dir/rewrite-$mode"
  printf 'old-proof\n' > "$proof_path"
  chmod "$mode" "$proof_path"
  write_secret_file "$proof_path" "new-proof-$mode"
  [[ "$(stat -f '%Lp' "$proof_path" 2>/dev/null || stat -c '%a' "$proof_path")" == "600" ]]
  [[ "$(< "$proof_path")" == "new-proof-$mode" ]]
done
write_secret_file "$proof_dir/rewrite-644" 'rotated-proof'
[[ "$(< "$proof_dir/rewrite-644")" == "rotated-proof" ]]
[[ -z "$(find "$proof_dir" -name '.oore-secret.*' -print -quit)" ]]

printf 'symlink-target\n' > "$proof_dir/symlink-target"
ln -s "$proof_dir/symlink-target" "$proof_dir/symlink-proof"
if proof_error="$(write_secret_file "$proof_dir/symlink-proof" 'replacement' 2>&1)"; then
  echo "[install-acceptance] expected symlink proof destination to fail" >&2
  exit 1
fi
[[ "$proof_error" == *"installer-owned regular file"* ]]
[[ "$(< "$proof_dir/symlink-target")" == "symlink-target" ]]

printf 'old-backend-proof\n' > "$proof_dir/backend-rewrite"
chmod 644 "$proof_dir/backend-rewrite"
OORE_INSTALL_ROOT="$proof_dir/install"
OORE_TRUSTED_PROXY_SHARED_SECRET='new-backend-proof'
OORE_TRUSTED_PROXY_SHARED_SECRET_FILE="$proof_dir/backend-rewrite"
ensure_backend_trusted_proxy_secret_file
[[ "$(< "$proof_dir/backend-rewrite")" == "new-backend-proof" ]]
[[ "$(stat -f '%Lp' "$proof_dir/backend-rewrite" 2>/dev/null || stat -c '%a' "$proof_dir/backend-rewrite")" == "600" ]]

printf 'backend-proof\n' > "$proof_dir/backend"
OORE_TRUSTED_PROXY_SHARED_SECRET=""
OORE_TRUSTED_PROXY_SHARED_SECRET_FILE="$proof_dir/backend"
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET=""
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE=""
ensure_frontend_secret_files
[[ -n "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]
[[ "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" != "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]
[[ -s "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]
[[ "$(stat -f '%Lp' "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" 2>/dev/null || stat -c '%a' "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE")" == "600" ]]
[[ "$(tr -d '[:space:]' < "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE")" != "backend-proof" ]]

OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET=""
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

pair_curl_args="$(mktemp)"
curl() {
  local payload=""
  payload="$(cat)"
  [[ "$payload" == '{"code":"fp_test-code"}' ]] || return 1
  printf '%s\n' "$*" > "$pair_curl_args"
  printf '%s\n' '{"backend_proof":"paired-backend-proof","user_email_header":"x-forwarded-email"}'
}
OORE_WEB_BACKEND_URL="https://backend.example/"
OORE_FRONTEND_PAIRING_CODE="fp_test-code"
OORE_TRUSTED_PROXY_SHARED_SECRET=""
OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER=""
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET=""
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE=""
pair_frontend_with_backend "$OORE_FRONTEND_PAIRING_CODE"
[[ "$OORE_TRUSTED_PROXY_SHARED_SECRET" == "paired-backend-proof" ]]
[[ "$OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER" == "x-forwarded-email" ]]
[[ -n "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET" ]]
[[ "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET" != "$OORE_TRUSTED_PROXY_SHARED_SECRET" ]]
[[ -z "$OORE_FRONTEND_PAIRING_CODE" ]]
grep -q -- 'https://backend.example/v1/frontend/pair' "$pair_curl_args"
if grep -q -- 'fp_test-code' "$pair_curl_args"; then
  echo '[install-acceptance] pairing code leaked into curl arguments' >&2
  exit 1
fi
rm -f "$pair_curl_args"
unset -f curl

bash "$ROOT_DIR/scripts/uninstall-acceptance.sh"

echo "[install-acceptance] passed"
