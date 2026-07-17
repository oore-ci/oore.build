#!/usr/bin/env bash
set -euo pipefail

OORE_VERSION="${OORE_VERSION:-latest}"
OORE_CHANNEL="${OORE_CHANNEL:-stable}"
OORE_INSTALL_MODE_WAS_SET=0
OORE_WEB_BACKEND_URL_WAS_SET=0
OORE_DAEMON_URL_WAS_SET=0
[[ -n "${OORE_INSTALL_MODE+x}" ]] && OORE_INSTALL_MODE_WAS_SET=1
[[ -n "${OORE_WEB_BACKEND_URL+x}" ]] && OORE_WEB_BACKEND_URL_WAS_SET=1
[[ -n "${OORE_DAEMON_URL+x}" ]] && OORE_DAEMON_URL_WAS_SET=1
OORE_INSTALL_MODE="${OORE_INSTALL_MODE:-auto}"
OORE_INSTALL_ROOT="${OORE_INSTALL_ROOT:-$HOME/.oore}"
OORE_GITHUB_REPO="${OORE_GITHUB_REPO:-oore-ci/oore.build}"
OORE_RELEASE_BASE_URL="${OORE_RELEASE_BASE_URL:-https://github.com/$OORE_GITHUB_REPO/releases/download}"
OORE_RELEASE_INDEX_BASE_URL="${OORE_RELEASE_INDEX_BASE_URL:-https://releases.oore.build}"
OORE_RELEASE_MANIFEST_URL="${OORE_RELEASE_MANIFEST_URL:-$OORE_RELEASE_INDEX_BASE_URL/latest/$OORE_CHANNEL.json}"
OORE_NONINTERACTIVE="${OORE_NONINTERACTIVE:-0}"
OORE_OPEN_BROWSER="${OORE_OPEN_BROWSER:-}"
OORE_START_DAEMON="${OORE_START_DAEMON:-}"
OORE_INSTALL_DAEMON_SERVICE="${OORE_INSTALL_DAEMON_SERVICE:-}"
OORE_DAEMON_LISTEN="${OORE_DAEMON_LISTEN:-}"
OORE_PUBLIC_URL="${OORE_PUBLIC_URL:-}"
OORE_WARPGATE_TICKET="${OORE_WARPGATE_TICKET:-}"
OORE_ARTIFACT_DELIVERY_URL="${OORE_ARTIFACT_DELIVERY_URL:-}"
OORE_CORS_ORIGINS="${OORE_CORS_ORIGINS:-}"
OORE_ENABLE_LINGER="${OORE_ENABLE_LINGER:-}"
OORE_HOSTED_UI="${OORE_HOSTED_UI:-https://ci.oore.build}"
OORE_SETUP_OWNER_EMAIL="${OORE_SETUP_OWNER_EMAIL:-}"
OORE_SETUP_PROXY_PRESET="${OORE_SETUP_PROXY_PRESET:-generic}"
OORE_SETUP_USER_EMAIL_HEADER="${OORE_SETUP_USER_EMAIL_HEADER:-}"
OORE_TRUSTED_PROXY_SHARED_SECRET="${OORE_TRUSTED_PROXY_SHARED_SECRET:-}"
OORE_TRUSTED_PROXY_SHARED_SECRET_FILE="${OORE_TRUSTED_PROXY_SHARED_SECRET_FILE:-}"
OORE_TRUSTED_PROXY_CIDRS="${OORE_TRUSTED_PROXY_CIDRS:-}"
OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER="${OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER:-}"
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET="${OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET:-}"
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE="${OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE:-}"
OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER="${OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER:-x-oore-web-trusted-proxy-secret}"
OORE_FRONTEND_PAIRING_CODE="${OORE_FRONTEND_PAIRING_CODE:-}"
OORE_DAEMON_URL="${OORE_DAEMON_URL:-http://127.0.0.1:8787}"
OORE_WEB_BACKEND_URL="${OORE_WEB_BACKEND_URL:-$OORE_DAEMON_URL}"
OORE_LOCAL_WEB_MODE="${OORE_LOCAL_WEB_MODE:-}"
OORE_LOCAL_WEB_LISTEN="${OORE_LOCAL_WEB_LISTEN:-127.0.0.1:4173}"

BIN_DIR="$OORE_INSTALL_ROOT/bin"
LOG_DIR="$OORE_INSTALL_ROOT/logs"
DAEMON_LOG="$LOG_DIR/oored.log"
DAEMON_PID_FILE="$OORE_INSTALL_ROOT/oored.pid"
WEB_LOG="$LOG_DIR/oore-web.log"
WEB_PID_FILE="$OORE_INSTALL_ROOT/oore-web.pid"
WEB_DIST_DIR="$OORE_INSTALL_ROOT/web-dist"
WEB_BINARY="$BIN_DIR/oore-web"
WEB_LAUNCH_AGENT_LABEL="build.oore.oore-web"
WEB_LAUNCH_AGENT_PLIST="$HOME/Library/LaunchAgents/$WEB_LAUNCH_AGENT_LABEL.plist"
WEB_SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
WEB_SYSTEMD_SERVICE_NAME="oore-web.service"
WEB_SYSTEMD_SERVICE_FILE="$WEB_SYSTEMD_USER_DIR/$WEB_SYSTEMD_SERVICE_NAME"
DAEMON_URL="$OORE_DAEMON_URL"
WEB_BACKEND_URL="$OORE_WEB_BACKEND_URL"
LOCAL_WEB_URL=""
RELEASE_TAG=""
RELEASE_VERSION=""
RELEASE_ARCH=""
RELEASE_OS=""
RESOLVED_CHANNEL=""
TMP_DIR=""
CURRENT_STEP=0
TOTAL_STEPS=5
BACKEND_SETUP_INITIALIZED=0
DAEMON_HEALTH_REACHABLE=0
DAEMON_STARTED=0
UI_RESET=""
UI_BOLD=""
UI_DIM=""
UI_ACCENT=""
UI_SUCCESS=""
UI_WARNING=""
UI_ERROR=""
OORE_ADVANCED=0
OORE_NO_OPEN=0

print_help() {
  cat <<'EOF'
Oore CI installer

Usage:
  ./scripts/install.sh
  ./scripts/install.sh --advanced
  ./scripts/install.sh --no-open
  ./scripts/install.sh --help

Environment overrides:
  OORE_VERSION               Release tag or "latest" (default: latest)
  OORE_CHANNEL               Release channel for latest resolution: stable|beta|alpha (default: stable)
  OORE_INSTALL_MODE          Install mode: auto|all|backend|frontend (default: auto; full is a legacy alias for all)
  OORE_INSTALL_ROOT          Install root (default: ~/.oore)
  OORE_NONINTERACTIVE        Non-interactive mode (true/false)
  OORE_OPEN_BROWSER          Open the local web root after install (true/false; defaults to true only for interactive local installs)
  OORE_DAEMON_LISTEN         Daemon listen address for all/backend installs (default: from OORE_DAEMON_URL)
  OORE_START_DAEMON          Start daemon in non-interactive mode (true/false)
  OORE_INSTALL_DAEMON_SERVICE Install oored as a launchd service in all/backend mode (true/false)
  OORE_PUBLIC_URL            Browser-visible HTTPS origin for remote access
  OORE_WARPGATE_TICKET       Optional Warpgate access ticket for iOS OTA installs
  OORE_ARTIFACT_DELIVERY_URL Optional token-only HTTPS origin for artifact installs behind an auth proxy
  OORE_CORS_ORIGINS          Comma-separated allowed browser origins (default: OORE_PUBLIC_URL when set)
  OORE_DAEMON_URL            Daemon URL used by all/backend setup helpers (default: http://127.0.0.1:8787)
  OORE_WEB_BACKEND_URL       Backend URL proxied by oore-web (default: OORE_DAEMON_URL)
  OORE_FRONTEND_PAIRING_CODE Short-lived code from `oore frontend invite`
  OORE_LOCAL_WEB_MODE        Local web behavior in non-interactive mode: off|run|login
  OORE_LOCAL_WEB_LISTEN      Local web listen address (default: 127.0.0.1:4173)
  OORE_ENABLE_LINGER         Enable systemd lingering for Linux frontend login service (true/false)
  OORE_HOSTED_UI             Hosted UI URL (default: https://ci.oore.build)
  OORE_SETUP_OWNER_EMAIL     Initial owner email to prefill for Trusted Proxy setup
  OORE_SETUP_PROXY_PRESET    Trusted Proxy preset: generic|warpgate|custom (default: generic)
  OORE_SETUP_USER_EMAIL_HEADER Custom Trusted Proxy email header when preset=custom
  OORE_TRUSTED_PROXY_SHARED_SECRET Shared secret injected by proxy/oore-web for Trusted Proxy mode
  OORE_TRUSTED_PROXY_SHARED_SECRET_FILE File containing the Trusted Proxy shared secret
  OORE_TRUSTED_PROXY_CIDRS  Comma-separated proxy/frontend peer CIDRs allowed to send Trusted Proxy identity
  OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER Header oore-web may forward after upstream proof
  OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET Secret your auth proxy sends to oore-web before identity headers are forwarded
  OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE File containing the auth proxy -> oore-web proof secret
  OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER Header carrying the auth proxy -> oore-web proof secret
  OORE_GITHUB_REPO           GitHub repo used to download assets (default: oore-ci/oore.build)
  OORE_RELEASE_BASE_URL      Release asset base URL (default: GitHub Releases download base)
  OORE_RELEASE_INDEX_BASE_URL Static release index origin (default: https://releases.oore.build)
  OORE_RELEASE_MANIFEST_URL  Latest channel manifest override (default: <index>/latest/<channel>.json)
EOF
}

is_default_local_install() {
  [[ "$RELEASE_OS" == "darwin" && "$OORE_ADVANCED" -eq 0 && "$OORE_INSTALL_MODE" == "all" ]]
}

should_open_browser() {
  [[ "$OORE_NO_OPEN" -eq 0 ]] || return 1

  if [[ -n "$OORE_OPEN_BROWSER" ]]; then
    normalize_bool "$OORE_OPEN_BROWSER"
    return $?
  fi

  ! is_noninteractive
}

report_component_failure() {
  local component="$1"
  local log_path="$2"
  local retry_command="$3"
  local expected_url="$4"

  log "$component failed. Logs: $log_path"
  log "Retry: $retry_command"
  log "Expected URL: $expected_url"
}

step() {
  CURRENT_STEP=$((CURRENT_STEP + 1))
  printf '%b[%d/%d]%b %-28s' "$UI_BOLD$UI_ACCENT" "$CURRENT_STEP" "$TOTAL_STEPS" "$UI_RESET" "$1"
}

step_done() {
  printf '%b%s%b\n' "$UI_SUCCESS" "$1" "$UI_RESET"
}

log() {
  printf '%b[oore-install]%b %s\n' "$UI_BOLD$UI_ACCENT" "$UI_RESET" "$*"
}

die() {
  printf '%b[oore-install] ERROR:%b %s\n' "$UI_BOLD$UI_ERROR" "$UI_RESET" "$*" >&2
  exit 1
}

has_prompt_tty() {
  if [[ ! -r /dev/tty || ! -w /dev/tty ]]; then
    return 1
  fi

  if ! (: >/dev/tty) 2>/dev/null; then
    return 1
  fi

  return 0
}

init_ui_theme() {
  if [[ -n "${NO_COLOR:-}" || "${TERM:-}" == "dumb" ]]; then
    return 0
  fi

  if [[ -t 1 ]] || [[ -t 2 ]] || has_prompt_tty; then
    UI_RESET=$'\033[0m'
    UI_BOLD=$'\033[1m'
    UI_DIM=$'\033[2m\033[38;2;120;113;108m'
    UI_ACCENT=$'\033[38;2;217;119;6m'
    UI_SUCCESS=$'\033[38;2;245;158;11m'
    UI_WARNING=$'\033[38;2;251;191;36m'
    UI_ERROR=$'\033[38;2;220;38;38m'
  fi
}

print_ascii_banner() {
  printf '%b' "$UI_BOLD$UI_ACCENT"
  cat <<'EOF'
   ____   ____  ____  ______      _________
  / __ \ / __ \/ __ \/ ____/     / ____/  _/
 / / / // / / / /_/ / __/       / /    / /  
/ /_/ // /_/ / _, _/ /___      / /____/ /   
\____/ \____/_/ |_/_____/      \____/___/   CI
EOF
  printf '%b\n' "$UI_RESET"
}

print_install_welcome() {
  printf '\n'
  print_ascii_banner
  printf '%bOore CI Installer%b\n' "$UI_BOLD$UI_ACCENT" "$UI_RESET"
  printf '%b----------------------------------------%b\n' "$UI_DIM" "$UI_RESET"
  printf '  Prompting:     %s\n' "$(ui_prompt_mode)"
  printf '  Install root:  %s\n' "$OORE_INSTALL_ROOT"
  if [[ "$OORE_VERSION" == "latest" ]]; then
    printf '  Release:       latest (%s channel)\n' "$OORE_CHANNEL"
  else
    printf '  Release:       %s\n' "$OORE_VERSION"
  fi
  printf '%b----------------------------------------%b\n' "$UI_DIM" "$UI_RESET"
}

print_install_summary() {
  printf '\n%bInstall configuration%b\n' "$UI_BOLD$UI_ACCENT" "$UI_RESET"
  printf '%b----------------------------------------%b\n' "$UI_DIM" "$UI_RESET"
  printf '  Mode:          %s\n' "$OORE_INSTALL_MODE"
  if is_daemon_install; then
    printf '  Daemon listen: %s\n' "$OORE_DAEMON_LISTEN"
    if [[ -n "$OORE_PUBLIC_URL" ]]; then
      printf '  Public URL:    %s\n' "$OORE_PUBLIC_URL"
    fi
    if [[ -n "$OORE_ARTIFACT_DELIVERY_URL" ]]; then
      printf '  Delivery URL:  %s\n' "$OORE_ARTIFACT_DELIVERY_URL"
    fi
    if [[ -n "$OORE_CORS_ORIGINS" ]]; then
      printf '  CORS origins:  %s\n' "$OORE_CORS_ORIGINS"
    fi
    if [[ -n "$OORE_SETUP_OWNER_EMAIL" ]]; then
      printf '  Setup owner:   %s\n' "$OORE_SETUP_OWNER_EMAIL"
      printf '  Proxy preset:  %s\n' "$OORE_SETUP_PROXY_PRESET"
      if [[ "$OORE_SETUP_PROXY_PRESET" == "custom" ]]; then
        printf '  Email header:  %s\n' "$OORE_SETUP_USER_EMAIL_HEADER"
      fi
      if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET" ]]; then
        printf '  Proxy secret:  configured\n'
      elif [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
        printf '  Proxy secret:  file configured\n'
      fi
    fi
  fi
  if [[ "$OORE_VERSION" == "latest" ]]; then
    printf '  Release:       latest (%s channel)\n' "$OORE_CHANNEL"
  else
    printf '  Release:       %s\n' "$OORE_VERSION"
  fi
  if [[ "$OORE_INSTALL_MODE" == "frontend" ]]; then
    printf '  Backend URL:   %s\n' "$WEB_BACKEND_URL"
    printf '  Web listen:    %s\n' "$OORE_LOCAL_WEB_LISTEN"
    if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET" || -n "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
      printf '  Proxy secret:  configured\n'
      printf '  Identity hdr:  %s\n' "${OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER:-$(setup_header_for_preset "$OORE_SETUP_PROXY_PRESET")}"
    fi
    if [[ -n "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET" || -n "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
      printf '  Upstream auth: configured\n'
    fi
  fi
  printf '  Hosted setup:  %s\n' "$OORE_HOSTED_UI"
  printf '%b----------------------------------------%b\n' "$UI_DIM" "$UI_RESET"
}

print_prompt_section() {
  local title="$1"
  local help="${2:-}"

  if is_noninteractive || ! has_prompt_tty; then
    return 0
  fi

  printf '\n%b%s%b\n' "$UI_BOLD$UI_ACCENT" "$title" "$UI_RESET" > /dev/tty
  if [[ -n "$help" ]]; then
    printf '%b%s%b\n' "$UI_DIM" "$help" "$UI_RESET" > /dev/tty
  fi
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

xml_escape() {
  local value="$1"
  value="${value//&/&amp;}"
  value="${value//</&lt;}"
  value="${value//>/&gt;}"
  value="${value//\"/&quot;}"
  value="${value//\'/&apos;}"
  printf '%s' "$value"
}

systemd_env_quote() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//\$/\\\$}"
  value="${value//\`/\\\`}"
  printf '"%s"' "$value"
}

write_secret_file() (
  local path="$1"
  local value="$2"
  local dir tmp mode

  dir="$(dirname "$path")"
  mkdir -p "$dir"
  if [[ -e "$path" || -L "$path" ]]; then
    [[ -f "$path" && ! -L "$path" && -O "$path" ]] \
      || die "Secret destination must be an installer-owned regular file: $path"
  fi
  umask 077
  tmp="$(mktemp "$dir/.oore-secret.XXXXXX")"
  trap 'rm -f "$tmp"' EXIT HUP INT TERM
  printf '%s\n' "$value" > "$tmp"
  chmod 600 "$tmp"
  mv -f "$tmp" "$path"
  trap - EXIT HUP INT TERM

  mode="$(stat -f '%Lp' "$path" 2>/dev/null || stat -c '%a' "$path" 2>/dev/null)" \
    || die "Failed to inspect secret destination: $path"
  [[ -f "$path" && ! -L "$path" && -O "$path" && "$mode" == "600" ]] \
    || die "Secret destination has unsafe ownership or permissions: $path"
)

trusted_proxy_secret_file_path() {
  printf '%s' "${OORE_TRUSTED_PROXY_SHARED_SECRET_FILE:-$OORE_INSTALL_ROOT/trusted-proxy-shared-secret}"
}

upstream_trusted_proxy_secret_file_path() {
  printf '%s' "${OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE:-$OORE_INSTALL_ROOT/oore-web-upstream-trusted-proxy-secret}"
}

ensure_backend_trusted_proxy_secret_file() {
  local path
  path="$(trusted_proxy_secret_file_path)"

  if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET" ]]; then
    write_secret_file "$path" "$OORE_TRUSTED_PROXY_SHARED_SECRET"
  elif [[ ! -s "$path" ]]; then
    OORE_TRUSTED_PROXY_SHARED_SECRET="$(generate_shared_secret)"
    write_secret_file "$path" "$OORE_TRUSTED_PROXY_SHARED_SECRET"
    log "Generated Trusted Proxy shared secret: $path"
  fi

  OORE_TRUSTED_PROXY_SHARED_SECRET_FILE="$path"
}

ensure_frontend_secret_files() {
  if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET" ]]; then
    OORE_TRUSTED_PROXY_SHARED_SECRET_FILE="$(trusted_proxy_secret_file_path)"
    write_secret_file "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" "$OORE_TRUSTED_PROXY_SHARED_SECRET"
  fi

  if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" && -z "$OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER" ]]; then
    OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER="$(setup_header_for_preset "$OORE_SETUP_PROXY_PRESET")"
  fi

  if [[ -n "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET" ]]; then
    OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE="$(upstream_trusted_proxy_secret_file_path)"
    write_secret_file "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET"
  fi

  if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
    [[ -s "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]] \
      || die "Backend Trusted Proxy proof file is missing or empty: $OORE_TRUSTED_PROXY_SHARED_SECRET_FILE"
    if [[ -z "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
      OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET="$(generate_shared_secret)"
      OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE="$(upstream_trusted_proxy_secret_file_path)"
      write_secret_file "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET"
      log "Generated auth-proxy proof: $OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE"
    fi
    [[ -s "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" ]] \
      || die "Auth-proxy proof file is missing or empty: $OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE"
    local backend_proof upstream_proof
    backend_proof="$(< "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE")"
    upstream_proof="$(< "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE")"
    backend_proof="${backend_proof#"${backend_proof%%[![:space:]]*}"}"
    backend_proof="${backend_proof%"${backend_proof##*[![:space:]]}"}"
    upstream_proof="${upstream_proof#"${upstream_proof%%[![:space:]]*}"}"
    upstream_proof="${upstream_proof%"${upstream_proof##*[![:space:]]}"}"
    [[ -n "$backend_proof" ]] || die 'Backend Trusted Proxy proof file is empty after trimming whitespace.'
    [[ -n "$upstream_proof" ]] || die 'Auth-proxy proof file is empty after trimming whitespace.'
    if [[ "$backend_proof" == "$upstream_proof" ]]; then
      die 'Backend and auth-proxy proof files must contain different values.'
    fi
    unset backend_proof upstream_proof
  elif [[ -n "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
    die 'Auth-proxy proof requires a backend Trusted Proxy proof. Set OORE_TRUSTED_PROXY_SHARED_SECRET or OORE_TRUSTED_PROXY_SHARED_SECRET_FILE.'
  fi
}

pair_frontend_with_backend() {
  local code="$1"
  local response=""
  local backend_proof=""
  local email_header=""

  [[ "$code" == fp_* ]] || die 'Frontend pairing code must start with fp_.'
  response="$(printf '{\"code\":\"%s\"}' "$code" | \
    curl -fsS --connect-timeout 10 --max-time 30 \
      -H 'content-type: application/json' \
      --data-binary @- \
      "${OORE_WEB_BACKEND_URL%/}/v1/frontend/pair")" \
    || die 'Frontend pairing failed. Create a new code on the Mac with: oore frontend invite'

  backend_proof="$(printf '%s' "$response" | sed -n 's/.*"backend_proof"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
  email_header="$(printf '%s' "$response" | sed -n 's/.*"user_email_header"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
  [[ -n "$backend_proof" && -n "$email_header" ]] \
    || die 'Frontend pairing returned an invalid response.'

  OORE_TRUSTED_PROXY_SHARED_SECRET="$backend_proof"
  OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER="$email_header"
  if [[ -z "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET" && -z "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
    OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET="$(generate_shared_secret)"
  fi
  OORE_FRONTEND_PAIRING_CODE=""
  log 'Frontend paired with backend.'
}

launchd_env_entry() {
  local key="$1"
  local value="$2"
  [[ -n "$value" ]] || return 0
  printf '      <key>%s</key>\n      <string>%s</string>\n' "$(xml_escape "$key")" "$(xml_escape "$value")"
}

launchd_environment_dict() {
  local entries=""
  entries="$(
    launchd_env_entry OORE_TRUSTED_PROXY_SHARED_SECRET_FILE "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE"
    launchd_env_entry OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER "$OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER"
    launchd_env_entry OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE"
    launchd_env_entry OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER"
  )"
  [[ -n "$entries" ]] || return 0
  printf '    <key>EnvironmentVariables</key>\n    <dict>\n%s\n    </dict>\n' "$entries"
}

systemd_env_line() {
  local key="$1"
  local value="$2"
  [[ -n "$value" ]] || return 0
  printf 'Environment=%s\n' "$(systemd_env_quote "$key=$value")"
}

systemd_secret_environment_lines() {
  systemd_env_line OORE_TRUSTED_PROXY_SHARED_SECRET_FILE "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE"
  systemd_env_line OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER "$OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER"
  systemd_env_line OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE"
  systemd_env_line OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER"
}

ensure_install_root_writable() {
  if [[ -e "$OORE_INSTALL_ROOT" ]]; then
    [[ -d "$OORE_INSTALL_ROOT" ]] || die "Install root exists but is not a directory: $OORE_INSTALL_ROOT"
    if [[ ! -w "$OORE_INSTALL_ROOT" ]]; then
      local owner
      owner="$(stat -f '%Su' "$OORE_INSTALL_ROOT" 2>/dev/null || echo unknown)"
      die "Install root is not writable: $OORE_INSTALL_ROOT (owner: $owner). If this was created by sudo/system setup, run: sudo chown -R \"$USER\":staff \"$OORE_INSTALL_ROOT\" or set OORE_INSTALL_ROOT to a user-owned path."
    fi
  else
    mkdir -p "$OORE_INSTALL_ROOT" \
      || die "Failed to create install root: $OORE_INSTALL_ROOT"
  fi
}

normalize_bool() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON)
      return 0
      ;;
    0|false|FALSE|no|NO|off|OFF)
      return 1
      ;;
    *)
      return 2
      ;;
  esac
}

validate_optional_bool_env() {
  local name="$1"
  local value="${2:-}"
  local status=0

  [[ -z "$value" ]] && return 0

  if normalize_bool "$value"; then
    return 0
  fi
  status="$?"
  if [[ "$status" -eq 1 ]]; then
    return 0
  fi

  die "$name must be one of: true,false,1,0,yes,no,on,off."
}

is_noninteractive() {
  normalize_bool "$OORE_NONINTERACTIVE"
}

ui_prompt_mode() {
  if is_noninteractive; then
    printf 'non-interactive'
  elif has_prompt_tty; then
    printf 'interactive'
  else
    printf 'auto-defaults (no TTY)'
  fi
}

prompt_yes_no() {
  local question="$1"
  local default="${2:-y}"
  local selected=""

  if [[ "$default" == "y" ]]; then
    selected="$(prompt_select "$question" "yes" "yes:Yes" "no:No")"
  else
    selected="$(prompt_select "$question" "no" "yes:Yes" "no:No")"
  fi

  [[ "$selected" == "yes" ]]
}

prompt_select() {
  local question="$1"
  local default_key="$2"
  shift 2

  local options=("$@")
  local option_count="${#options[@]}"
  local i=0
  local key=""
  local label=""
  local selected=""
  local default_index=1
  local answer=""

  [[ "$option_count" -gt 0 ]] || die "prompt_select requires at least one option."

  for ((i = 0; i < option_count; i++)); do
    key="${options[$i]%%:*}"
    label="${options[$i]#*:}"
    [[ "$label" != "$key" ]] || label="$key"

    if [[ -z "$default_key" ]]; then
      default_key="$key"
      default_index=$((i + 1))
      continue
    fi

    if [[ "$key" == "$default_key" ]]; then
      default_index=$((i + 1))
    fi
  done

  if is_noninteractive || ! has_prompt_tty; then
    printf '%s' "$default_key"
    return 0
  fi

  while true; do
    printf '\n%b%s%b\n' "$UI_BOLD" "$question" "$UI_RESET" > /dev/tty
    for ((i = 0; i < option_count; i++)); do
      key="${options[$i]%%:*}"
      label="${options[$i]#*:}"
      [[ "$label" != "$key" ]] || label="$key"
      if [[ "$key" == "$default_key" ]]; then
        printf '  %b%d)%b %s %b(default)%b\n' \
          "$UI_ACCENT" "$((i + 1))" "$UI_RESET" "$label" "$UI_DIM" "$UI_RESET" > /dev/tty
      else
        printf '  %b%d)%b %s\n' "$UI_ACCENT" "$((i + 1))" "$UI_RESET" "$label" > /dev/tty
      fi
    done
    printf '%bSelect an option [%d]:%b ' "$UI_DIM" "$default_index" "$UI_RESET" > /dev/tty

    if ! read -r answer < /dev/tty; then
      printf '%s' "$default_key"
      return 0
    fi

    if [[ -z "$answer" ]]; then
      printf '%s' "$default_key"
      return 0
    fi

    if [[ "$answer" =~ ^[0-9]+$ ]]; then
      if ((answer >= 1 && answer <= option_count)); then
        selected="${options[$((answer - 1))]%%:*}"
        printf '%s' "$selected"
        return 0
      fi
      printf '%bPlease enter a number between 1 and %d.%b\n' "$UI_WARNING" "$option_count" "$UI_RESET" > /dev/tty
      continue
    fi

    for ((i = 0; i < option_count; i++)); do
      key="${options[$i]%%:*}"
      if [[ "$answer" == "$key" ]] \
        || [[ "$key" == "yes" && "$answer" =~ ^([Yy]|[Yy][Ee][Ss])$ ]] \
        || [[ "$key" == "no" && "$answer" =~ ^([Nn]|[Nn][Oo])$ ]]; then
        printf '%s' "$key"
        return 0
      fi
    done

    printf '%bPlease enter a valid option number.%b\n' "$UI_WARNING" "$UI_RESET" > /dev/tty
  done
}

prompt_text() {
  local question="$1"
  local default="${2:-}"
  local required="${3:-optional}"
  local answer=""
  local prompt_label="Enter value"

  if is_noninteractive || ! has_prompt_tty; then
    if [[ -z "$default" && "$required" == "required" ]]; then
      die "$question must be provided in non-interactive mode."
    fi
    printf '%s' "$default"
    return 0
  fi

  while true; do
    printf '\n%b%s%b\n' "$UI_BOLD" "$question" "$UI_RESET" > /dev/tty

    if [[ -n "$default" ]]; then
      printf '  %bdefault%b %s\n' "$UI_DIM" "$UI_RESET" "$default" > /dev/tty
      printf '%b%s [%s]:%b ' "$UI_DIM" "$prompt_label" "$default" "$UI_RESET" > /dev/tty
    elif [[ "$required" == "required" ]]; then
      printf '%b%s:%b ' "$UI_DIM" "$prompt_label" "$UI_RESET" > /dev/tty
    else
      printf '%b%s (optional):%b ' "$UI_DIM" "$prompt_label" "$UI_RESET" > /dev/tty
    fi

    if ! read -r answer < /dev/tty; then
      answer="$default"
    fi

    if [[ -z "$answer" ]]; then
      answer="$default"
    fi

    if [[ -n "$answer" || "$required" != "required" ]]; then
      printf '%s' "$answer"
      return 0
    fi

    printf '%bPlease enter a value.%b\n' "$UI_WARNING" "$UI_RESET" > /dev/tty
  done
}

ensure_dependency() {
  local cmd="$1"

  if have_cmd "$cmd"; then
    return 0
  fi

  die "$cmd is required. Install it and rerun."
}

detect_arch() {
  case "$(uname -m)" in
    arm64|aarch64)
      RELEASE_ARCH="arm64"
      ;;
    x86_64|amd64)
      RELEASE_ARCH="x86_64"
      ;;
    *)
      die "Unsupported architecture: $(uname -m). Supported architectures: arm64, x86_64."
      ;;
  esac
}

detect_os() {
  case "$(uname -s)" in
    Darwin)
      RELEASE_OS="darwin"
      ;;
    Linux)
      RELEASE_OS="linux"
      ;;
    *)
      die "Unsupported operating system: $(uname -s). Backend install supports macOS; frontend install supports macOS and Linux."
      ;;
  esac
}

validate_install_mode() {
  case "${OORE_INSTALL_MODE:-}" in
    auto|all|backend|frontend|full)
      return 0
      ;;
    *)
      die 'OORE_INSTALL_MODE must be one of: auto,all,backend,frontend. The old full value is accepted as an all-in-one alias.'
      ;;
  esac
}

normalize_install_mode() {
  if [[ "${OORE_INSTALL_MODE:-}" == "full" ]]; then
    OORE_INSTALL_MODE="all"
  fi
}

is_daemon_install() {
  [[ "${OORE_INSTALL_MODE:-}" == "all" || "${OORE_INSTALL_MODE:-}" == "backend" ]]
}

is_web_install() {
  [[ "${OORE_INSTALL_MODE:-}" == "all" || "${OORE_INSTALL_MODE:-}" == "frontend" ]]
}

validate_channel() {
  case "${OORE_CHANNEL:-}" in
    stable|alpha|beta)
      return 0
      ;;
    *)
      die 'OORE_CHANNEL must be one of: stable,alpha,beta.'
      ;;
  esac
}

validate_setup_proxy_preset() {
  case "${OORE_SETUP_PROXY_PRESET:-}" in
    generic|warpgate|custom)
      return 0
      ;;
    *)
      die 'OORE_SETUP_PROXY_PRESET must be one of: generic,warpgate,custom.'
      ;;
  esac
}

setup_header_for_preset() {
  case "${1:-generic}" in
    generic)
      printf 'x-oore-user-email'
      ;;
    warpgate)
      printf 'x-warpgate-username'
      ;;
    custom)
      printf '%s' "$OORE_SETUP_USER_EMAIL_HEADER"
      ;;
    *)
      die "Unsupported trusted proxy preset: $1"
      ;;
  esac
}

generate_shared_secret() {
  if have_cmd openssl; then
    openssl rand -hex 32
    return 0
  fi
  od -An -N32 -tx1 /dev/urandom | tr -d ' \n'
}

url_to_host_port() {
  local raw="$1"
  local without_scheme="${raw#http://}"
  without_scheme="${without_scheme#https://}"
  without_scheme="${without_scheme%%/*}"
  without_scheme="${without_scheme%%\?*}"
  without_scheme="${without_scheme%%#*}"
  printf '%s' "$without_scheme"
}

daemon_url_from_listen() {
  local listen="$1"
  if [[ "$listen" == http://* || "$listen" == https://* ]]; then
    printf '%s' "${listen%/}"
  else
    printf 'http://%s' "$listen"
  fi
}

normalize_runtime_config() {
  if [[ -z "$OORE_DAEMON_LISTEN" ]]; then
    OORE_DAEMON_LISTEN="$(url_to_host_port "$DAEMON_URL")"
  fi

  if [[ "$OORE_DAEMON_LISTEN" == http://* || "$OORE_DAEMON_LISTEN" == https://* ]]; then
    OORE_DAEMON_LISTEN="$(url_to_host_port "$OORE_DAEMON_LISTEN")"
  fi

  if [[ -z "$OORE_DAEMON_LISTEN" ]]; then
    OORE_DAEMON_LISTEN="127.0.0.1:8787"
  fi

  if [[ "$OORE_DAEMON_URL_WAS_SET" -eq 0 ]]; then
    DAEMON_URL="$(daemon_url_from_listen "$OORE_DAEMON_LISTEN")"
    OORE_DAEMON_URL="$DAEMON_URL"
  fi

  if [[ -z "$OORE_CORS_ORIGINS" && -n "$OORE_PUBLIC_URL" ]]; then
    OORE_CORS_ORIGINS="$OORE_PUBLIC_URL"
  fi
}

configure_install_mode() {
  normalize_install_mode

  if [[ "$OORE_ADVANCED" -eq 0 && "$RELEASE_OS" == "darwin" && "$OORE_INSTALL_MODE" == "auto" ]]; then
    OORE_INSTALL_MODE="all"
    return 0
  fi

  if [[ "$OORE_INSTALL_MODE" == "auto" ]]; then
    case "$RELEASE_OS" in
      linux)
        OORE_INSTALL_MODE="frontend"
        ;;
      darwin)
        if [[ "$OORE_INSTALL_MODE_WAS_SET" -eq 0 && ! is_noninteractive && has_prompt_tty ]]; then
          OORE_INSTALL_MODE="$(
            prompt_select \
              "What role should this machine run?" \
              "all" \
              "all:Backend + CLI + embedded runner + local web" \
              "backend:Backend daemon + CLI + embedded runner only" \
              "frontend:Frontend-only web proxy"
          )"
        else
          OORE_INSTALL_MODE="all"
        fi
        ;;
    esac
  fi

  normalize_install_mode
}

configure_backend_install() {
  is_daemon_install || return 0

  if [[ "$RELEASE_OS" != "darwin" ]]; then
    die 'Oore CI V1 backend installer currently supports macOS only.'
  fi

  if [[ -z "$OORE_DAEMON_LISTEN" ]]; then
    OORE_DAEMON_LISTEN="$(url_to_host_port "$DAEMON_URL")"
  fi
  [[ -n "$OORE_DAEMON_LISTEN" ]] || OORE_DAEMON_LISTEN="127.0.0.1:8787"

  if is_default_local_install; then
    OORE_DAEMON_LISTEN="127.0.0.1:8787"
    OORE_DAEMON_URL="http://127.0.0.1:8787"
    DAEMON_URL="$OORE_DAEMON_URL"
    OORE_WEB_BACKEND_URL="$DAEMON_URL"
    WEB_BACKEND_URL="$DAEMON_URL"
    OORE_INSTALL_DAEMON_SERVICE=true
    OORE_START_DAEMON=true
    return 0
  fi

  if ! is_noninteractive && has_prompt_tty; then
    local listen_default="$OORE_DAEMON_LISTEN"
    local access_choice=""

    print_prompt_section \
      "Backend setup" \
      "Configure how oored should bind and whether browser clients will call it directly."

    OORE_DAEMON_LISTEN="$(
      prompt_text \
        "Daemon listen address. Use host:port. Keep 127.0.0.1:8787 for same-host use; bind a private interface when another machine must reach oored." \
        "$listen_default" \
        "required"
    )"

    if [[ -n "$OORE_PUBLIC_URL" || -n "$OORE_CORS_ORIGINS" ]]; then
      access_choice="direct"
    else
      access_choice="$(
        prompt_select \
          "How will browsers reach the backend API?" \
          "proxy" \
          "proxy:Through oore-web or a reverse proxy on the same origin" \
          "direct:Directly from another browser origin (configure External Access now)"
      )"
    fi

    if [[ "$access_choice" == "direct" ]]; then
      OORE_PUBLIC_URL="$(
        prompt_text \
          "Public HTTPS URL for the backend API. Leave blank only if you will set it later in External Access." \
          "$OORE_PUBLIC_URL" \
          "optional"
      )"

      if [[ -z "$OORE_CORS_ORIGINS" && -n "$OORE_PUBLIC_URL" ]]; then
        OORE_CORS_ORIGINS="$OORE_PUBLIC_URL"
      fi

      OORE_CORS_ORIGINS="$(
        prompt_text \
          "Allowed frontend origins for direct browser API calls, comma-separated." \
          "$OORE_CORS_ORIGINS" \
          "optional"
      )"
    fi

    if [[ -z "$OORE_INSTALL_DAEMON_SERVICE" ]]; then
      local service_choice=""
      service_choice="$(
        prompt_select \
          "Run oored as a launchd service?" \
          "yes" \
          "yes:Install and start launchd service (recommended)" \
          "no:Start one background process only" \
          "skip:Do not start now"
      )"
      case "$service_choice" in
        yes)
          OORE_INSTALL_DAEMON_SERVICE=true
          OORE_START_DAEMON=true
          ;;
        no)
          OORE_INSTALL_DAEMON_SERVICE=false
          OORE_START_DAEMON=true
          ;;
        skip)
          OORE_INSTALL_DAEMON_SERVICE=false
          OORE_START_DAEMON=false
          ;;
      esac
    fi
  fi

  normalize_runtime_config
}

configure_frontend_install() {
  is_web_install || return 0

  if is_default_local_install; then
    OORE_LOCAL_WEB_LISTEN="127.0.0.1:4173"
    OORE_LOCAL_WEB_MODE=login
    WEB_BACKEND_URL="$DAEMON_URL"
    resolve_local_web_url
    return 0
  fi

  if [[ "$OORE_INSTALL_MODE" == "frontend" ]] && ! is_noninteractive && has_prompt_tty; then
    local backend_default="$WEB_BACKEND_URL"
    if [[ "$OORE_WEB_BACKEND_URL_WAS_SET" -eq 0 && "$OORE_DAEMON_URL_WAS_SET" -eq 0 ]]; then
      backend_default=""
    fi

    WEB_BACKEND_URL="$(
      prompt_text \
        "Backend daemon URL reachable from this frontend host, for example http://10.0.0.20:8787 or https://ci-api.example.com." \
        "$backend_default" \
        "required"
    )"
    OORE_WEB_BACKEND_URL="$WEB_BACKEND_URL"

    OORE_LOCAL_WEB_LISTEN="$(
      prompt_text \
        "Local oore-web listen address. Keep loopback when a reverse proxy runs on this host; bind a private interface only when you intentionally expose oore-web directly." \
        "$OORE_LOCAL_WEB_LISTEN" \
        "required"
    )"

    if [[ -z "$OORE_LOCAL_WEB_MODE" ]]; then
      OORE_LOCAL_WEB_MODE="$(
        prompt_select \
          "Run oore-web automatically as a user service?" \
          "login" \
          "login:Enable systemd/launchd service (recommended)" \
          "run:Start it now only" \
          "off:Install only"
      )"
    fi

    if [[ "$RELEASE_OS" == "linux" && "$OORE_LOCAL_WEB_MODE" == "login" && -z "$OORE_ENABLE_LINGER" ]]; then
      local linger_choice=""
      linger_choice="$(
        prompt_select \
          "Enable systemd lingering so oore-web survives logout/reboot?" \
          "yes" \
          "yes:Enable lingering now" \
          "no:Show command later"
      )"
      if [[ "$linger_choice" == "yes" ]]; then
        OORE_ENABLE_LINGER=true
      else
        OORE_ENABLE_LINGER=false
      fi
    fi

    if [[ -z "$OORE_TRUSTED_PROXY_SHARED_SECRET" && -z "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
      if [[ -z "$OORE_FRONTEND_PAIRING_CODE" ]]; then
        OORE_FRONTEND_PAIRING_CODE="$(
          prompt_text \
            "Frontend pairing code from 'oore frontend invite' on the backend Mac. Leave blank only for OIDC or manual proof setup." \
            "" \
            "optional"
        )"
      fi
      if [[ -n "$OORE_FRONTEND_PAIRING_CODE" ]]; then
        pair_frontend_with_backend "$OORE_FRONTEND_PAIRING_CODE"
      else
        OORE_TRUSTED_PROXY_SHARED_SECRET="$(
          prompt_text \
            "Backend Trusted Proxy shared secret. Use the value from the backend host; leave blank to disable trusted-proxy identity forwarding here." \
            "$OORE_TRUSTED_PROXY_SHARED_SECRET" \
            "optional"
        )"
      fi
    fi

    if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET" || -n "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
      if [[ -z "$OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER" ]]; then
        OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER="$(
          prompt_text \
            "Trusted Proxy user email header that your auth proxy sets for oore-web." \
            "$(setup_header_for_preset "$OORE_SETUP_PROXY_PRESET")" \
            "required"
        )"
      fi

      if [[ -z "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET" && -z "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
        OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET="$(
          prompt_text \
            "Auth proxy -> oore-web proof secret. Configure your auth proxy to send this in ${OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER} with the user email header." \
            "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET" \
            "required"
        )"
      fi
    fi
  elif [[ "$OORE_INSTALL_MODE" == "frontend" ]]; then
    if [[ "$OORE_WEB_BACKEND_URL_WAS_SET" -eq 0 && "$OORE_DAEMON_URL_WAS_SET" -eq 0 ]]; then
      die 'Frontend-only non-interactive install requires OORE_WEB_BACKEND_URL, for example http://<backend-host>:8787.'
    fi
    if [[ -n "$OORE_FRONTEND_PAIRING_CODE" ]]; then
      pair_frontend_with_backend "$OORE_FRONTEND_PAIRING_CODE"
    fi
  fi

  [[ "$OORE_INSTALL_MODE" == "frontend" ]] || return 0

  ensure_frontend_secret_files
  WEB_BACKEND_URL="$OORE_WEB_BACKEND_URL"
  resolve_local_web_url
}

configure_setup_prefill() {
  is_daemon_install || return 0

  is_default_local_install && return 0

  if ! is_noninteractive && has_prompt_tty; then
    print_prompt_section \
      "First-run setup defaults" \
      "Optional backend-owned setup initialization. Leave blank if you will use Local Only or OIDC."

    OORE_SETUP_OWNER_EMAIL="$(
      prompt_text \
        "Initial owner email for Trusted Proxy setup." \
        "$OORE_SETUP_OWNER_EMAIL" \
        "optional"
    )"

    OORE_SETUP_OWNER_EMAIL="$(printf '%s' "$OORE_SETUP_OWNER_EMAIL" | tr '[:upper:]' '[:lower:]')"

    if [[ -n "$OORE_SETUP_OWNER_EMAIL" ]]; then
      OORE_SETUP_PROXY_PRESET="$(
        prompt_select \
          "Trusted Proxy identity header preset?" \
          "$OORE_SETUP_PROXY_PRESET" \
          "generic:Generic proxy (x-oore-user-email)" \
          "warpgate:Warpgate (x-warpgate-username)" \
          "custom:Custom header"
      )"
      if [[ "$OORE_SETUP_PROXY_PRESET" == "custom" ]]; then
        OORE_SETUP_USER_EMAIL_HEADER="$(
          prompt_text \
            "Trusted Proxy user email header." \
            "$OORE_SETUP_USER_EMAIL_HEADER" \
            "required"
        )"
      fi
      if [[ -z "$OORE_TRUSTED_PROXY_SHARED_SECRET" && -z "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
        OORE_TRUSTED_PROXY_SHARED_SECRET="$(
          prompt_text \
            "Trusted Proxy shared secret for backend/frontend proxy hop. Leave blank to generate one." \
            "$OORE_TRUSTED_PROXY_SHARED_SECRET" \
            "optional"
        )"
      fi
      if [[ -z "$OORE_TRUSTED_PROXY_CIDRS" ]]; then
        OORE_TRUSTED_PROXY_CIDRS="$(
          prompt_text \
            "Trusted proxy/frontend peer CIDRs allowed to send identity headers. Use comma-separated CIDRs; leave blank for loopback-only." \
            "$OORE_TRUSTED_PROXY_CIDRS" \
            "optional"
        )"
      fi
    fi
  fi

  validate_setup_proxy_preset

  if [[ "$OORE_SETUP_PROXY_PRESET" == "custom" && -n "$OORE_SETUP_OWNER_EMAIL" && -z "$OORE_SETUP_USER_EMAIL_HEADER" ]]; then
    die 'OORE_SETUP_USER_EMAIL_HEADER is required when OORE_SETUP_PROXY_PRESET=custom and OORE_SETUP_OWNER_EMAIL is set.'
  fi
}

infer_channel_from_tag() {
  local tag="${1:-}"
  if echo "$tag" | grep -q -- '-alpha\.'; then
    printf 'alpha'
  elif echo "$tag" | grep -q -- '-beta\.'; then
    printf 'beta'
  else
    printf 'stable'
  fi
}

resolve_release_tag() {
  local tag=""
  if [[ "$OORE_VERSION" == "latest" ]]; then
    local manifest_file="$TMP_DIR/latest.json"
    curl -fsSL --retry 3 --connect-timeout 10 --max-time 60 --output "$manifest_file" "$OORE_RELEASE_MANIFEST_URL" \
      || die "Unable to fetch latest $OORE_CHANNEL release manifest: $OORE_RELEASE_MANIFEST_URL"

    tag="$(sed -n 's/.*"tag"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest_file" | head -n1)"
    # Preserve compatibility with custom manifests using GitHub's older field name.
    if [[ -z "$tag" ]]; then
      tag="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest_file" | head -n1)"
    fi
    [[ -n "$tag" ]] || die "Unable to parse tag from release manifest: $OORE_RELEASE_MANIFEST_URL"
  else
    if [[ "$OORE_VERSION" == v* ]]; then
      tag="$OORE_VERSION"
    else
      tag="v$OORE_VERSION"
    fi
  fi

  if [[ "$tag" != v* ]]; then
    tag="v$tag"
  fi

  RELEASE_TAG="$tag"
  RELEASE_VERSION="${RELEASE_TAG#v}"
  if [[ -z "$RELEASE_VERSION" ]]; then
    die "Failed to normalize release version from tag: $RELEASE_TAG"
  fi

  if [[ "$OORE_VERSION" == "latest" ]]; then
    RESOLVED_CHANNEL="$OORE_CHANNEL"
  else
    RESOLVED_CHANNEL="$(infer_channel_from_tag "$RELEASE_TAG")"
  fi
}

release_archive_name() {
  case "$OORE_INSTALL_MODE" in
    all|backend)
      printf 'oore_%s_darwin_%s.tar.gz' "$RELEASE_VERSION" "$RELEASE_ARCH"
      ;;
    frontend)
      printf 'oore-web_%s_%s_%s.tar.gz' "$RELEASE_VERSION" "$RELEASE_OS" "$RELEASE_ARCH"
      ;;
    *)
      die "Unsupported install mode: $OORE_INSTALL_MODE"
      ;;
  esac
}

download_release_assets() {
  local archive_name
  local checksum_name="oore_${RELEASE_VERSION}_checksums.txt"
  local base_url="${OORE_RELEASE_BASE_URL%/}/$RELEASE_TAG"
  archive_name="$(release_archive_name)"
  local archive_url="$base_url/$archive_name"
  local checksum_url="$base_url/$checksum_name"

  log "Downloading release assets for $RELEASE_TAG ($OORE_INSTALL_MODE/$RELEASE_OS/$RELEASE_ARCH)..."
  curl -fsSL --retry 3 --connect-timeout 10 --max-time 600 --output "$TMP_DIR/$archive_name" "$archive_url" \
    || die "Failed to download release archive: $archive_url"
  curl -fsSL --retry 3 --connect-timeout 10 --max-time 60 --output "$TMP_DIR/$checksum_name" "$checksum_url" \
    || die "Failed to download checksum file: $checksum_url"
}

compute_sha256() {
  local file="$1"
  if have_cmd shasum; then
    shasum -a 256 "$file" | awk '{ print $1 }'
    return 0
  fi
  if have_cmd sha256sum; then
    sha256sum "$file" | awk '{ print $1 }'
    return 0
  fi
  die "shasum or sha256sum is required to verify release checksums."
}

urlencode() {
  local LC_ALL=C
  local value="$1"
  local out=""
  local char=""
  local hex=""
  local i

  for ((i = 0; i < ${#value}; i++)); do
    char="${value:i:1}"
    case "$char" in
      [a-zA-Z0-9.~_-])
        out+="$char"
        ;;
      *)
        printf -v hex '%%%02X' "'$char"
        out+="$hex"
        ;;
    esac
  done

  printf '%s' "$out"
}

setup_prefill_query() {
  local query=""
  local sep=""
  local header=""

  if [[ -n "$OORE_SETUP_OWNER_EMAIL" ]]; then
    query+="setup_owner_email=$(urlencode "$OORE_SETUP_OWNER_EMAIL")"
    sep="&"
    query+="${sep}proxy_preset=$(urlencode "$OORE_SETUP_PROXY_PRESET")"
    header="$(setup_header_for_preset "$OORE_SETUP_PROXY_PRESET")"
    if [[ -n "$header" ]]; then
      query+="&user_email_header=$(urlencode "$header")"
    fi
  fi

  printf '%s' "$query"
}

setup_url_with_prefill() {
  local base="$1"
  local query=""
  query="$(setup_prefill_query)"

  if [[ -z "$query" ]]; then
    printf '%s' "$base"
  elif [[ "$base" == *\?* ]]; then
    printf '%s&%s' "$base" "$query"
  else
    printf '%s?%s' "$base" "$query"
  fi
}

setup_url_with_backend_and_prefill() {
  local base="$1"
  local query="backend=$(urlencode "$DAEMON_URL")"
  local prefill=""
  prefill="$(setup_prefill_query)"
  if [[ -n "$prefill" ]]; then
    query+="&$prefill"
  fi
  printf '%s?%s' "$base" "$query"
}

curl_quick() {
  curl -fsS --connect-timeout 2 --max-time 5 "$@"
}

verify_archive_checksum() {
  local archive_name
  local checksum_name="oore_${RELEASE_VERSION}_checksums.txt"
  local expected=""
  local actual=""
  archive_name="$(release_archive_name)"

  expected="$(
    awk -v file="$archive_name" '$2 == file { print $1 }' "$TMP_DIR/$checksum_name"
  )"
  [[ -n "$expected" ]] || die "Checksum entry for $archive_name not found in $checksum_name."

  actual="$(compute_sha256 "$TMP_DIR/$archive_name")"
  [[ -n "$actual" ]] || die "Failed to compute checksum for $archive_name."

  if [[ "$actual" != "$expected" ]]; then
    die "Checksum mismatch for $archive_name (expected $expected, got $actual)."
  fi

  log "Checksum verified for $archive_name."
}

install_executable() {
  local source="$1"
  local destination="$2"
  local staged="${destination}.install.$$"
  install -m 0755 "$source" "$staged"
  mv -f "$staged" "$destination"
}

install_binaries() {
  local archive_name
  local extract_dir="$TMP_DIR/extract"
  archive_name="$(release_archive_name)"

  mkdir -p "$extract_dir"
  tar -xzf "$TMP_DIR/$archive_name" -C "$extract_dir"

  if is_daemon_install; then
    [[ -f "$extract_dir/bin/oored" ]] || die "Release archive is missing bin/oored."
    [[ -f "$extract_dir/bin/oore" ]] || die "Release archive is missing bin/oore."
  fi
  if is_web_install; then
    [[ -f "$extract_dir/bin/oore-web" ]] || die "Release archive is missing bin/oore-web."
    [[ -d "$extract_dir/web-dist" ]] || die "Release archive is missing web-dist."
  fi
  [[ -f "$extract_dir/VERSION" ]] || die "Release archive is missing VERSION."

  mkdir -p "$BIN_DIR" "$LOG_DIR"
  if is_daemon_install; then
    install_executable "$extract_dir/bin/oored" "$BIN_DIR/oored"
    install_executable "$extract_dir/bin/oore" "$BIN_DIR/oore"
  fi
  if is_web_install; then
    install_executable "$extract_dir/bin/oore-web" "$WEB_BINARY"
    rm -rf "$WEB_DIST_DIR"
    cp -R "$extract_dir/web-dist" "$WEB_DIST_DIR"
  else
    rm -f "$WEB_BINARY"
    rm -rf "$WEB_DIST_DIR"
  fi

  cp "$extract_dir/VERSION" "$OORE_INSTALL_ROOT/VERSION"
  printf '%s\n' "$OORE_INSTALL_MODE" > "$OORE_INSTALL_ROOT/INSTALL_MODE"
  if [[ -n "${RESOLVED_CHANNEL:-}" ]]; then
    printf '%s\n' "$RESOLVED_CHANNEL" > "$OORE_INSTALL_ROOT/CHANNEL"
  fi
  printf '%s\n' "$OORE_GITHUB_REPO" > "$OORE_INSTALL_ROOT/GITHUB_REPO"
  if [[ -f "$extract_dir/LICENSE" ]]; then
    cp "$extract_dir/LICENSE" "$OORE_INSTALL_ROOT/LICENSE"
  fi
}

persist_cli_daemon_url() {
  is_daemon_install || return 0
  [[ -x "$BIN_DIR/oore" ]] || return 0

  if "$BIN_DIR/oore" config set daemon_url "$DAEMON_URL" >/dev/null 2>&1; then
    log "Saved CLI daemon URL: $DAEMON_URL"
  else
    log "Could not save CLI daemon URL automatically. Run: oore config set daemon_url $DAEMON_URL"
  fi
}

ensure_on_path() {
  # Already on PATH — nothing to do
  case ":$PATH:" in
    *":$BIN_DIR:"*) return 0 ;;
  esac

  # Detect shell config file
  local shell_rc=""
  case "$(basename "${SHELL:-/bin/zsh}")" in
    zsh)  shell_rc="$HOME/.zshrc" ;;
    bash) shell_rc="$HOME/.bashrc" ;;
    *)    shell_rc="$HOME/.profile" ;;
  esac

  local path_line="export PATH=\"$BIN_DIR:\$PATH\""

  # Check if already added in a previous install
  if [[ -f "$shell_rc" ]] && grep -qF "$BIN_DIR" "$shell_rc" 2>/dev/null; then
    # Already in rc file but not active in this shell session
    export PATH="$BIN_DIR:$PATH"
    return 0
  fi

  if is_noninteractive || prompt_yes_no "Add $BIN_DIR to your PATH (in $shell_rc)?" 'y'; then
    printf '\n# Oore CI\n%s\n' "$path_line" >> "$shell_rc"
    export PATH="$BIN_DIR:$PATH"
    log "Added $BIN_DIR to PATH in $shell_rc"
    log "Run 'source $shell_rc' or open a new terminal to use 'oore' and 'oored' directly."
  fi
}

start_daemon() {
  mkdir -p "$LOG_DIR"

  if curl_quick "$DAEMON_URL/healthz" >/dev/null 2>&1; then
    log "A healthy daemon is already running on $DAEMON_URL."
    DAEMON_HEALTH_REACHABLE=1
    DAEMON_STARTED=1
    return 0
  fi

  log "Starting oored in background on $OORE_DAEMON_LISTEN..."
  nohup "$BIN_DIR/oored" run --listen "$OORE_DAEMON_LISTEN" >"$DAEMON_LOG" 2>&1 &
  echo "$!" > "$DAEMON_PID_FILE"

  local i
  for i in $(seq 1 15); do
    if curl_quick "$DAEMON_URL/healthz" >/dev/null 2>&1; then
      log 'Daemon is healthy.'
      DAEMON_HEALTH_REACHABLE=1
      DAEMON_STARTED=1
      return 0
    fi
    sleep 1
  done

  if [[ -f "$DAEMON_PID_FILE" ]] && kill -0 "$(cat "$DAEMON_PID_FILE")" >/dev/null 2>&1; then
    log "Daemon process started, but this host could not reach $DAEMON_URL/healthz. Continuing; check logs if clients cannot connect."
    DAEMON_STARTED=1
    return 0
  fi

  report_component_failure \
    "oored" \
    "$DAEMON_LOG" \
    "$BIN_DIR/oored run --listen $OORE_DAEMON_LISTEN" \
    "$DAEMON_URL/healthz"
  return 1
}

install_daemon_service() {
  local cmd=("$BIN_DIR/oored" "install-service" "--listen" "$OORE_DAEMON_LISTEN")
  local retry_cmd="$BIN_DIR/oored install-service --listen $OORE_DAEMON_LISTEN"

  if [[ "$OORE_INSTALL_MODE" == "backend" ]]; then
    ensure_dependency sudo
    local service_user
    service_user="$(id -un)"
    "$BIN_DIR/oored" uninstall-service >/dev/null 2>&1 || true
    cmd=(sudo "$BIN_DIR/oored" "install-service" "--system" "--user" "$service_user" "--listen" "$OORE_DAEMON_LISTEN")
    cmd+=("--env" "HOME=$HOME")
    retry_cmd="sudo $BIN_DIR/oored install-service --system --user $service_user --listen $OORE_DAEMON_LISTEN --env HOME=$HOME"
  fi

  if [[ -n "$OORE_PUBLIC_URL" ]]; then
    cmd+=("--env" "OORE_PUBLIC_URL=$OORE_PUBLIC_URL")
  fi
  if [[ -n "$OORE_WARPGATE_TICKET" ]]; then
    cmd+=("--env" "OORE_WARPGATE_TICKET=$OORE_WARPGATE_TICKET")
  fi
  if [[ -n "$OORE_ARTIFACT_DELIVERY_URL" ]]; then
    cmd+=("--env" "OORE_ARTIFACT_DELIVERY_URL=$OORE_ARTIFACT_DELIVERY_URL")
  fi
  if [[ -n "$OORE_CORS_ORIGINS" ]]; then
    cmd+=("--env" "OORE_CORS_ORIGINS=$OORE_CORS_ORIGINS")
  fi
  cmd+=("--env" "RUST_LOG=${RUST_LOG:-info}")

  if ! "${cmd[@]}"; then
    report_component_failure \
      "oored launchd service" \
      "$DAEMON_LOG" \
      "$retry_cmd" \
      "$DAEMON_URL/healthz"
    return 1
  fi

  local i
  for i in $(seq 1 15); do
    if curl_quick "$DAEMON_URL/healthz" >/dev/null 2>&1; then
      log 'Daemon service is healthy.'
      DAEMON_HEALTH_REACHABLE=1
      DAEMON_STARTED=1
      return 0
    fi
    sleep 1
  done

  if is_default_local_install; then
    report_component_failure \
      "oored launchd service" \
      "$DAEMON_LOG" \
      "$retry_cmd" \
      "$DAEMON_URL/healthz"
    return 1
  fi

  log "Daemon service was installed, but this host could not reach $DAEMON_URL/healthz. Continuing; check logs if clients cannot connect."
  DAEMON_STARTED=1
  return 0
}

is_already_configured() {
  local status_json
  status_json="$(curl_quick "$DAEMON_URL/v1/public/setup-status" 2>/dev/null)" || return 1
  # Check if is_configured is true in the JSON response
  echo "$status_json" | grep -q '"is_configured"[[:space:]]*:[[:space:]]*true'
}

generate_setup_token() {
  if ! curl_quick "$DAEMON_URL/healthz" >/dev/null 2>&1; then
    log "Daemon is not healthy. Skipping token generation. Check logs: $DAEMON_LOG"
    return 1
  fi

  # Skip if instance is already configured (reinstall/upgrade)
  if is_already_configured; then
    log "Instance is already configured. Skipping token generation."
    return 0
  fi

  "$BIN_DIR/oore" setup token --ttl 15m \
    || die "Failed to generate setup token. Check daemon logs: $DAEMON_LOG"
}

initialize_backend_setup_if_requested() {
  is_daemon_install || return 0
  [[ -n "$OORE_SETUP_OWNER_EMAIL" ]] || return 0

  local header
  header="$(setup_header_for_preset "$OORE_SETUP_PROXY_PRESET")"
  [[ -n "$header" ]] || die 'Trusted Proxy user email header is required.'

  ensure_backend_trusted_proxy_secret_file

  local args=(
    setup init
    --mode trusted-proxy
    --owner-email "$OORE_SETUP_OWNER_EMAIL"
    --user-email-header "$header"
  )
  local cidr=""
  if [[ -n "$OORE_TRUSTED_PROXY_CIDRS" ]]; then
    IFS=',' read -ra cidr_values <<< "$OORE_TRUSTED_PROXY_CIDRS"
    for cidr in "${cidr_values[@]}"; do
      cidr="$(printf '%s' "$cidr" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
      [[ -n "$cidr" ]] || continue
      args+=(--trusted-proxy-cidr "$cidr")
    done
  fi

  log "Initializing backend setup in Remote Trusted Proxy mode..."
  env -u OORE_TRUSTED_PROXY_SHARED_SECRET \
    OORE_TRUSTED_PROXY_SHARED_SECRET_FILE="$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" \
    "$BIN_DIR/oore" "${args[@]}" >/dev/null \
    || die 'Failed to initialize backend Trusted Proxy setup.'
  BACKEND_SETUP_INITIALIZED=1
}

is_localhost_backend() {
  case "$DAEMON_URL" in
    http://localhost:*|http://localhost|http://127.0.0.1:*|http://127.0.0.1)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

should_install_daemon_service() {
  normalize_bool "${OORE_INSTALL_DAEMON_SERVICE:-false}"
}

validate_local_web_mode() {
  case "${OORE_LOCAL_WEB_MODE:-}" in
    ""|off|run|login)
      return 0
      ;;
    *)
      die 'OORE_LOCAL_WEB_MODE must be one of: off,run,login.'
      ;;
  esac
}

local_web_url_from_listen() {
  local listen="${1:-$OORE_LOCAL_WEB_LISTEN}"
  if [[ "$listen" == http://* || "$listen" == https://* ]]; then
    printf '%s' "${listen%/}"
    return 0
  fi

  if [[ "$listen" == *:* ]]; then
    printf 'http://%s' "$listen"
    return 0
  fi

  die "OORE_LOCAL_WEB_LISTEN must be host:port or URL (got: $listen)"
}

resolve_local_web_url() {
  LOCAL_WEB_URL="$(local_web_url_from_listen "$OORE_LOCAL_WEB_LISTEN")"
}

has_local_web_bundle() {
  [[ -x "$WEB_BINARY" && -f "$WEB_DIST_DIR/index.html" ]]
}

is_local_web_healthy() {
  curl_quick "${LOCAL_WEB_URL}/__oore_web_healthz" >/dev/null 2>&1
}

preflight_local_web_listen() {
  # A healthy existing oore-web instance already owns this address.
  is_local_web_healthy && return 0

  local port="${OORE_LOCAL_WEB_LISTEN##*:}"
  [[ "$port" =~ ^[0-9]+$ ]] || die "OORE_LOCAL_WEB_LISTEN must include a numeric port (got: $OORE_LOCAL_WEB_LISTEN)"

  local listeners=""
  if have_cmd lsof; then
    listeners="$(lsof -nP -iTCP:"$port" -sTCP:LISTEN -t 2>/dev/null || true)"
  elif have_cmd ss; then
    if ss -H -ltn "sport = :$port" 2>/dev/null | grep -q .; then
      listeners=1
    fi
  else
    die "Cannot check whether $OORE_LOCAL_WEB_LISTEN is available: install lsof or ss."
  fi

  [[ -z "$listeners" ]] || die "Cannot start oore-web: $OORE_LOCAL_WEB_LISTEN is already in use. Set OORE_LOCAL_WEB_LISTEN to an available host:port."
}

start_local_web() {
  if ! has_local_web_bundle; then
    log "Bundled local web UI not found in this release."
    return 1
  fi

  preflight_local_web_listen

  mkdir -p "$LOG_DIR"

  if is_local_web_healthy; then
    log "Local web UI is already running at $LOCAL_WEB_URL."
    return 0
  fi

  local web_cmd=(
    "$WEB_BINARY"
    --listen "$OORE_LOCAL_WEB_LISTEN"
    --backend-url "$WEB_BACKEND_URL"
    --dist-dir "$WEB_DIST_DIR"
  )
  local web_env=()
  [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]] && web_env+=(OORE_TRUSTED_PROXY_SHARED_SECRET_FILE="$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE")
  [[ -n "$OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER" ]] && web_env+=(OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER="$OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER")
  [[ -n "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE" ]] && web_env+=(OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE="$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE")
  [[ -n "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER" ]] && web_env+=(OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER="$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER")
  nohup env \
    -u OORE_TRUSTED_PROXY_SHARED_SECRET \
    -u OORE_WEB_TRUSTED_PROXY_SHARED_SECRET \
    -u OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET \
    "${web_env[@]}" "${web_cmd[@]}" >"$WEB_LOG" 2>&1 &
  echo "$!" > "$WEB_PID_FILE"

  local i
  for i in $(seq 1 15); do
    if is_local_web_healthy; then
      log "Local web UI is healthy at $LOCAL_WEB_URL."
      return 0
    fi
    sleep 1
  done

  report_component_failure \
    "oore-web" \
    "$WEB_LOG" \
    "$WEB_BINARY --listen $OORE_LOCAL_WEB_LISTEN --backend-url $WEB_BACKEND_URL --dist-dir $WEB_DIST_DIR" \
    "$LOCAL_WEB_URL"
  return 1
}

install_local_web_launch_agent() {
  if ! has_local_web_bundle; then
    log "Cannot install launch agent: bundled local web UI is unavailable."
    return 1
  fi

  preflight_local_web_listen

  mkdir -p "$HOME/Library/LaunchAgents" "$LOG_DIR"
  cat > "$WEB_LAUNCH_AGENT_PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>Label</key>
    <string>$WEB_LAUNCH_AGENT_LABEL</string>
    <key>ProgramArguments</key>
    <array>
      <string>$WEB_BINARY</string>
      <string>--listen</string>
      <string>$OORE_LOCAL_WEB_LISTEN</string>
      <string>--backend-url</string>
      <string>$WEB_BACKEND_URL</string>
      <string>--dist-dir</string>
      <string>$WEB_DIST_DIR</string>
    </array>
$(launchd_environment_dict)
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$WEB_LOG</string>
    <key>StandardErrorPath</key>
    <string>$WEB_LOG</string>
  </dict>
</plist>
EOF

  local uid
  uid="$(id -u)"
  launchctl bootout "gui/$uid/$WEB_LAUNCH_AGENT_LABEL" >/dev/null 2>&1 || true

  if ! launchctl bootstrap "gui/$uid" "$WEB_LAUNCH_AGENT_PLIST" >/dev/null 2>&1; then
    # Fallback for older macOS launchctl variants.
    if ! launchctl load -w "$WEB_LAUNCH_AGENT_PLIST" >/dev/null 2>&1; then
      report_component_failure \
        "oore-web launch agent" \
        "$WEB_LOG" \
        "launchctl load -w $WEB_LAUNCH_AGENT_PLIST" \
        "$LOCAL_WEB_URL"
      return 1
    fi
  fi

  launchctl kickstart -k "gui/$uid/$WEB_LAUNCH_AGENT_LABEL" >/dev/null 2>&1 \
    || true

  log "Installed launch-at-login local web UI agent: $WEB_LAUNCH_AGENT_LABEL"
  return 0
}

install_local_web_systemd_user_service() {
  if ! has_local_web_bundle; then
    log "Cannot install systemd user service: bundled web UI is unavailable."
    return 1
  fi

  preflight_local_web_listen

  if ! have_cmd systemctl; then
    log "Cannot install systemd user service: systemctl is unavailable."
    return 1
  fi

  enable_linux_lingering || true

  mkdir -p "$WEB_SYSTEMD_USER_DIR" "$LOG_DIR"
  cat > "$WEB_SYSTEMD_SERVICE_FILE" <<EOF
[Unit]
Description=Oore CI frontend launcher
After=network-online.target

[Service]
Type=simple
ExecStart=$WEB_BINARY --listen $OORE_LOCAL_WEB_LISTEN --backend-url $WEB_BACKEND_URL --dist-dir $WEB_DIST_DIR
Restart=on-failure
RestartSec=3
Environment=NODE_ENV=production
$(systemd_secret_environment_lines)

[Install]
WantedBy=default.target
EOF

  systemctl --user daemon-reload
  systemctl --user enable --now "$WEB_SYSTEMD_SERVICE_NAME"
  log "Installed systemd user service: $WEB_SYSTEMD_SERVICE_NAME"
  return 0
}

enable_linux_lingering() {
  [[ "$(uname -s)" == "Linux" ]] || return 0
  [[ "${OORE_LOCAL_WEB_MODE:-}" == "login" ]] || return 0
  have_cmd loginctl || return 0

  if loginctl show-user "$USER" -p Linger --value 2>/dev/null | grep -q '^yes$'; then
    log "systemd lingering is already enabled for $USER."
    return 0
  fi

  case "${OORE_ENABLE_LINGER:-}" in
    1|true|TRUE|yes|YES|on|ON)
      ;;
    0|false|FALSE|no|NO|off|OFF)
      log "systemd lingering was not enabled. To keep oore-web alive after logout: sudo loginctl enable-linger $USER"
      return 0
      ;;
    "")
      log "To keep oore-web alive after logout/reboot, run: sudo loginctl enable-linger $USER"
      return 0
      ;;
    *)
      die 'OORE_ENABLE_LINGER must be one of: true,false,1,0,yes,no,on,off.'
      ;;
  esac

  if loginctl enable-linger "$USER" >/dev/null 2>&1; then
    log "Enabled systemd lingering for $USER."
    return 0
  fi

  if have_cmd sudo; then
    log "Enabling systemd lingering for $USER may ask for sudo."
    if sudo loginctl enable-linger "$USER"; then
      log "Enabled systemd lingering for $USER."
      return 0
    fi
  fi

  log "Could not enable lingering automatically. Run: sudo loginctl enable-linger $USER"
  return 0
}

install_local_web_autostart() {
  case "$(uname -s)" in
    Darwin)
      install_local_web_launch_agent
      ;;
    Linux)
      install_local_web_systemd_user_service
      ;;
    *)
      log "Autostart is not supported on this OS."
      return 1
      ;;
  esac
}

configure_local_web_noninteractive() {
  case "${OORE_LOCAL_WEB_MODE:-}" in
    ""|off)
      return 0
      ;;
    run)
      start_local_web || die "Failed to start local web UI in non-interactive mode."
      ;;
    login)
      install_local_web_autostart \
        || die "Failed to install local web autostart in non-interactive mode."
      start_local_web
      ;;
    *)
      die 'OORE_LOCAL_WEB_MODE must be one of: off,run,login.'
      ;;
  esac
}

handle_local_backend_onboarding() {
  local next_action=""
  local open_choice=""
  local launch_choice=""

  printf '\n'
  log "Backend is running locally at $DAEMON_URL."
  log "Recommended first-run path is local-only setup."
  log ""
  log "Local-first options:"
  if has_local_web_bundle; then
    log "  1. Local web UI: $LOCAL_WEB_URL/setup"
    log "     Opens directly to your local setup wizard."
  else
    log "  1. Local web UI: not bundled in this release build."
  fi
  log "  2. CLI setup:    $BIN_DIR/oore setup"
  log ""
  log "Remote mode (optional later):"
  log "  - Expose backend over HTTPS and open: ${OORE_HOSTED_UI}/setup?backend=<https-url>"
  log "  - Example tunnel command: cloudflared tunnel --url $DAEMON_URL"

  if has_local_web_bundle; then
    next_action="$(
      prompt_select \
        "How do you want to continue?" \
        "local_web" \
        "local_web:Start local web UI now (recommended)" \
        "cli_setup:Use CLI setup manually (oore setup)" \
        "hosted_setup:Open hosted setup URL (remote mode path)" \
        "skip:Skip for now"
    )"
  else
    next_action="$(
      prompt_select \
        "How do you want to continue?" \
        "cli_setup" \
        "cli_setup:Use CLI setup manually (oore setup)" \
        "hosted_setup:Open hosted setup URL (remote mode path)" \
        "skip:Skip for now"
    )"
  fi

  case "$next_action" in
    local_web)
      start_local_web || true
      if have_cmd open; then
        open_choice="$(
          prompt_select \
            "Open local setup UI in your browser now?" \
            "yes" \
            "yes:Open local web UI" \
            "no:Not now"
        )"
        if [[ "$open_choice" == "yes" ]]; then
          open "$(setup_url_with_prefill "${LOCAL_WEB_URL}/setup")" >/dev/null 2>&1 || true
        fi
      fi

      launch_choice="$(
        prompt_select \
          "Run local web UI automatically at login?" \
          "no" \
          "yes:Enable launch-at-login" \
          "no:Not now"
      )"
      if [[ "$launch_choice" == "yes" ]]; then
        install_local_web_autostart || log "Failed to install local web autostart."
      fi
      ;;
    hosted_setup)
      if have_cmd open; then
        open "${OORE_HOSTED_UI}/setup" >/dev/null 2>&1 || true
      else
        log "Open this URL in your browser: ${OORE_HOSTED_UI}/setup"
      fi
      ;;
    cli_setup|skip)
      :
      ;;
    *)
      :
      ;;
  esac
}

open_setup_ui() {
  if is_localhost_backend && is_local_web_healthy; then
    if ! have_cmd open; then
      log 'Cannot auto-open browser because the `open` command is unavailable.'
      return 1
    fi
    open "$(setup_url_with_prefill "${LOCAL_WEB_URL}/setup")" >/dev/null 2>&1 || true
    return 0
  fi

  # The hosted UI (HTTPS) cannot make requests to a local HTTP backend
  # due to browser mixed-content restrictions. Skip auto-open.
  if is_localhost_backend; then
    return 1
  fi

  if [[ -n "$OORE_PUBLIC_URL" ]]; then
    if ! have_cmd open; then
      log "Open this URL in your browser: $(setup_url_with_prefill "${OORE_PUBLIC_URL%/}/setup")"
      return 1
    fi
    open "$(setup_url_with_prefill "${OORE_PUBLIC_URL%/}/setup")" >/dev/null 2>&1 || true
    return 0
  fi

  if [[ "$DAEMON_URL" != https://* ]]; then
    log "Setup UI was not auto-opened because hosted HTTPS UI cannot call a plain HTTP backend directly."
    log "Finish frontend/reverse-proxy setup first, then open its /setup URL."
    return 1
  fi

  if ! have_cmd open; then
    log 'Cannot auto-open browser because the `open` command is unavailable.'
    return 1
  fi
  local setup_url
  setup_url="$(setup_url_with_backend_and_prefill "${OORE_HOSTED_UI}/setup")"
  open "$setup_url" >/dev/null 2>&1 || true
  return 0
}

open_links() {
  if ! have_cmd open; then
    log 'Cannot auto-open links because the `open` command is unavailable.'
    return 1
  fi
  open "$OORE_HOSTED_UI" >/dev/null 2>&1 || true
  open 'https://docs.oore.build' >/dev/null 2>&1 || true
  return 0
}

print_setup_prefill_next_steps() {
  [[ -n "$OORE_SETUP_OWNER_EMAIL" ]] || return 0

  printf '\nTrusted Proxy setup:\n'
  printf '  Owner email:  %s\n' "$OORE_SETUP_OWNER_EMAIL"
  printf '  Proxy preset: %s\n' "$OORE_SETUP_PROXY_PRESET"
  if [[ "$OORE_SETUP_PROXY_PRESET" == "custom" ]]; then
    printf '  Email header: %s\n' "$OORE_SETUP_USER_EMAIL_HEADER"
  fi
  if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET" || -n "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
    printf '  Secret:       configured\n'
    if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
      printf '  Secret file:  %s\n' "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE"
    fi
    printf '  Proxy header: x-oore-trusted-proxy-secret\n'
  fi
  if [[ -n "$OORE_TRUSTED_PROXY_CIDRS" ]]; then
    printf '  Proxy CIDRs:  %s\n' "$OORE_TRUSTED_PROXY_CIDRS"
  fi
}

print_next_steps() {
  local daemon_running=false
  local local_web_running=false
  if curl_quick "$DAEMON_URL/healthz" >/dev/null 2>&1; then
    daemon_running=true
  fi
  if [[ "$DAEMON_STARTED" -eq 1 ]]; then
    daemon_running=true
  fi
  if [[ -n "$LOCAL_WEB_URL" ]] && is_local_web_healthy; then
    local_web_running=true
  fi

  printf '\n%bInstallation complete%b\n' "$UI_BOLD$UI_ACCENT" "$UI_RESET"
  printf '%b----------------------------------------%b\n' "$UI_DIM" "$UI_RESET"

  if [[ "$OORE_INSTALL_MODE" == "frontend" ]]; then
    printf 'Frontend is installed at %s\n' "$LOCAL_WEB_URL"
    printf 'Backend proxy target: %s\n\n' "$WEB_BACKEND_URL"
    if "$local_web_running"; then
      printf 'Frontend status: running\n'
      printf 'Verify frontend + backend: oore-web status --url %s\n' "$LOCAL_WEB_URL"
    else
      printf 'Start the frontend:\n'
      printf '  oore-web --listen %s --backend-url %s\n' "$OORE_LOCAL_WEB_LISTEN" "$WEB_BACKEND_URL"
    fi
    if [[ "$(uname -s)" == "Linux" && "$OORE_LOCAL_WEB_MODE" == "login" ]]; then
      printf '\nSystemd service:\n'
      printf '  systemctl --user status %s\n' "$WEB_SYSTEMD_SERVICE_NAME"
      printf '  sudo loginctl enable-linger %s   # only needed if not already enabled\n' "$USER"
    fi
    printf '\nPut your HTTPS reverse proxy in front of %s.\n' "$LOCAL_WEB_URL"
    printf 'In the UI, add an instance with Backend URL empty so browser API calls use this frontend proxy.\n'
    if [[ -n "$OORE_TRUSTED_PROXY_SHARED_SECRET_FILE" ]]; then
      printf 'Trusted Proxy identity headers are forwarded only when your auth proxy also sends %s.\n' "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER"
      printf 'Auth proxy proof file: %s\n' "$OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE"
      printf 'Keep the proof private; configure HAProxy to read it through your service-secret mechanism.\n'
    fi
    printf '\nDocs: https://docs.oore.build\n'
    return 0
  fi

  if "$daemon_running"; then
    if [[ "$DAEMON_HEALTH_REACHABLE" -eq 1 ]]; then
      printf 'Daemon is running at %s\n\n' "$DAEMON_URL"
    else
      printf 'Daemon service/process started. Health was not reachable from this host at %s.\n\n' "$DAEMON_URL"
    fi
    if should_install_daemon_service; then
      printf 'Daemon service: launchd enabled\n\n'
    else
      printf 'To keep the daemon running across login sessions:\n'
      printf '  oored install-service --listen %s\n\n' "$OORE_DAEMON_LISTEN"
    fi
    if [[ "$BACKEND_SETUP_INITIALIZED" -eq 1 ]]; then
      printf 'Setup is initialized. Sign in through your configured auth path.\n'
    elif is_default_local_install; then
      printf 'Open the local web UI and use loopback local login:\n'
      printf '  %s\n' "$LOCAL_WEB_URL"
    else
      printf 'Complete setup:\n'
      if has_local_web_bundle; then
        printf '  %s\n' "$(setup_url_with_prefill "${LOCAL_WEB_URL}/setup")"
        printf '  (or use CLI below)\n'
      fi
      printf '  oore setup                    # interactive CLI setup\n'
      printf '  oore setup token --ttl 15m     # generate a new bootstrap token\n'
    fi
    if has_local_web_bundle && "$local_web_running"; then
      printf '  local web status: running\n'
    elif has_local_web_bundle; then
      printf '  local web start:  oore-web --backend-url %s\n' "$DAEMON_URL"
    fi
    if [[ -n "$OORE_PUBLIC_URL" ]]; then
      printf '\nConfigured public setup URL:\n'
      printf '  %s\n' "$(setup_url_with_prefill "${OORE_PUBLIC_URL%/}/setup")"
    else
      printf '\nRemote mode (optional later, requires HTTPS backend):\n'
      printf '  %s\n' "$OORE_HOSTED_UI"
    fi
    print_setup_prefill_next_steps
  else
    printf 'Start the daemon:\n'
    printf '  oored run --listen %s\n\n' "$OORE_DAEMON_LISTEN"
    printf 'Or install it as a launch-at-login service:\n'
    printf '  oored install-service --listen %s\n\n' "$OORE_DAEMON_LISTEN"
    if [[ "$BACKEND_SETUP_INITIALIZED" -eq 1 ]]; then
      printf 'Setup is already initialized. After the daemon starts, sign in through your configured auth path.\n'
    else
      printf 'Then complete setup:\n'
      if has_local_web_bundle; then
        printf '  %s\n' "$(setup_url_with_prefill "${LOCAL_WEB_URL}/setup")"
        printf '  (or use CLI below)\n'
      fi
      printf '  oore setup                    # interactive CLI setup\n'
      printf '  oore setup token --ttl 15m     # generate a bootstrap token\n'
    fi
    if has_local_web_bundle; then
      printf '  local web start:  oore-web --backend-url %s\n' "$DAEMON_URL"
    fi
    printf '\nRemote mode (optional later, requires HTTPS backend):\n'
    printf '  %s\n' "$OORE_HOSTED_UI"
    print_setup_prefill_next_steps
  fi

  printf '\nDocs: https://docs.oore.build\n'
}

cleanup() {
  if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR"
  fi
}

main() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      -h|--help)
        print_help
        return 0
        ;;
      --advanced)
        OORE_ADVANCED=1
        ;;
      --no-open)
        OORE_NO_OPEN=1
        ;;
      *)
        die "Unknown argument: $1 (use --help)"
        ;;
    esac
    shift
  done

  trap cleanup EXIT
  init_ui_theme

  validate_local_web_mode
  validate_channel
  validate_optional_bool_env OORE_START_DAEMON "$OORE_START_DAEMON"
  validate_optional_bool_env OORE_INSTALL_DAEMON_SERVICE "$OORE_INSTALL_DAEMON_SERVICE"
  validate_optional_bool_env OORE_ENABLE_LINGER "$OORE_ENABLE_LINGER"
  validate_optional_bool_env OORE_OPEN_BROWSER "$OORE_OPEN_BROWSER"

  if normalize_bool "$OORE_NONINTERACTIVE"; then
    :
  else
    if [[ "$?" -eq 2 ]]; then
      die 'OORE_NONINTERACTIVE must be one of: 1,0,true,false,yes,no,on,off.'
    fi
  fi

  detect_os
  validate_install_mode
  print_install_welcome
  configure_install_mode
  validate_install_mode
  configure_backend_install
  configure_frontend_install
  configure_setup_prefill
  print_install_summary

  ensure_dependency curl
  ensure_dependency tar
  ensure_dependency awk
  ensure_dependency uname
  ensure_dependency mktemp
  if ! have_cmd shasum && ! have_cmd sha256sum; then
    die 'shasum or sha256sum is required.'
  fi

  ensure_install_root_writable

  # Step 1: Detect platform
  step "Detecting platform..."
  detect_arch
  step_done "$RELEASE_OS $RELEASE_ARCH"

  TMP_DIR="$(mktemp -d)"
  resolve_local_web_url

  # Step 2: Download
  resolve_release_tag
  step "Downloading $RELEASE_TAG..."
  download_release_assets
  step_done "$(release_archive_name)"

  # Step 3: Verify checksum
  step "Verifying checksum..."
  verify_archive_checksum
  step_done "SHA-256 verified"

  # Step 4: Install binaries
  step "Installing binaries..."
  install_binaries
  if [[ "$OORE_INSTALL_MODE" == "frontend" ]]; then
    step_done "$BIN_DIR/oore-web + web-dist"
  elif [[ "$OORE_INSTALL_MODE" == "backend" ]]; then
    step_done "$BIN_DIR/{oored,oore}"
  elif has_local_web_bundle; then
    step_done "$BIN_DIR/{oored,oore,oore-web}"
  else
    step_done "$BIN_DIR/{oored,oore}"
  fi

  ensure_on_path
  persist_cli_daemon_url

  if [[ "$OORE_INSTALL_MODE" == "frontend" ]]; then
    step "Configuring frontend..."
    configure_local_web_noninteractive
    if [[ -z "${OORE_LOCAL_WEB_MODE:-}" || "${OORE_LOCAL_WEB_MODE:-}" == "off" ]]; then
      step_done "installed (start with oore-web)"
    else
      step_done "$LOCAL_WEB_URL"
    fi
    print_next_steps
    return 0
  fi

  if is_noninteractive; then
    # Step 5: Non-interactive daemon handling
    if should_install_daemon_service; then
      step "Installing daemon service..."
      install_daemon_service || exit 1
      initialize_backend_setup_if_requested
      if is_default_local_install; then
        configure_local_web_noninteractive || exit 1
      fi
      if [[ "$DAEMON_HEALTH_REACHABLE" -eq 1 ]]; then
        step_done "$DAEMON_URL (launchd)"
      else
        step_done "launchd installed (health not reachable from this host)"
      fi
    elif [[ -n "$OORE_START_DAEMON" ]]; then
      if normalize_bool "$OORE_START_DAEMON"; then
        step "Starting daemon..."
        start_daemon || exit 1
        initialize_backend_setup_if_requested
        if is_localhost_backend; then
          configure_local_web_noninteractive
        fi
        if [[ "$DAEMON_HEALTH_REACHABLE" -eq 1 ]]; then
          step_done "$DAEMON_URL (healthy)"
        else
          step_done "started (health not reachable from this host)"
        fi
      else
        if [[ "$?" -eq 2 ]]; then
          die 'OORE_START_DAEMON must be one of: true,false,1,0,yes,no,on,off.'
        fi
        step "Starting daemon..."
        step_done "skipped (OORE_START_DAEMON=false)"
      fi
    else
      step "Starting daemon..."
      step_done "skipped (non-interactive default)"
    fi
  else
    # Step 5: Interactive daemon handling
    if should_install_daemon_service; then
      step "Installing daemon service..."
      if install_daemon_service; then
        initialize_backend_setup_if_requested
        if is_default_local_install; then
          configure_local_web_noninteractive
        fi
        daemon_started=0
      else
        daemon_started=1
      fi
    elif normalize_bool "${OORE_START_DAEMON:-true}"; then
      step "Starting daemon..."
      if start_daemon; then
        initialize_backend_setup_if_requested
        daemon_started=0
      else
        daemon_started=1
      fi
    else
      step "Starting daemon..."
      step_done "skipped"
      daemon_started=1
    fi

    if [[ "$daemon_started" -eq 0 ]]; then
      if [[ "$DAEMON_HEALTH_REACHABLE" -eq 1 ]]; then
        step_done "$DAEMON_URL (healthy)"
      else
        step_done "started (health not reachable from this host)"
      fi

      # Auto-generate bootstrap token if not already configured
      if [[ "$BACKEND_SETUP_INITIALIZED" -eq 1 ]]; then
        printf '\n'
        log "Backend setup was initialized by the installer."
      elif is_default_local_install; then
        printf '\n'
        log "Local web UI is ready. Loopback local login will complete first-run setup."
      elif ! is_already_configured; then
        printf '\n'
        generate_setup_token || true

        if is_localhost_backend; then
          handle_local_backend_onboarding
        else
          printf '\n'
          log "Backend install is done. Setup can continue from your frontend or HTTPS proxy."
          open_setup_ui || true
        fi
      else
        printf '\n'
        log "Instance is already configured."
      fi
    else
      step_done "failed (check $DAEMON_LOG)"
    fi
  fi

  print_next_steps

  if is_default_local_install && should_open_browser; then
    if is_local_web_healthy; then
      open "$LOCAL_WEB_URL" >/dev/null 2>&1 || log "Could not open browser. Open: $LOCAL_WEB_URL"
    else
      report_component_failure \
        "oore-web" \
        "$WEB_LOG" \
        "$WEB_BINARY --listen $OORE_LOCAL_WEB_LISTEN --backend-url $WEB_BACKEND_URL --dist-dir $WEB_DIST_DIR" \
        "$LOCAL_WEB_URL"
      return 1
    fi
  fi
}

main "$@"
