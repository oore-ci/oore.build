#!/usr/bin/env bash
set -euo pipefail

OORE_VERSION="${OORE_VERSION:-latest}"
OORE_INSTALL_ROOT="${OORE_INSTALL_ROOT:-$HOME/.oore}"
OORE_GITHUB_REPO="${OORE_GITHUB_REPO:-devaryakjha/oore.build}"
OORE_RELEASE_BASE_URL="${OORE_RELEASE_BASE_URL:-https://github.com/$OORE_GITHUB_REPO/releases/download}"
OORE_RELEASE_MANIFEST_URL="${OORE_RELEASE_MANIFEST_URL:-https://api.github.com/repos/$OORE_GITHUB_REPO/releases/latest}"
OORE_NONINTERACTIVE="${OORE_NONINTERACTIVE:-0}"
OORE_START_DAEMON="${OORE_START_DAEMON:-}"
OORE_HOSTED_UI="${OORE_HOSTED_UI:-https://ci.oore.build}"
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
DAEMON_URL="http://127.0.0.1:8787"
LOCAL_WEB_URL=""
RELEASE_TAG=""
RELEASE_VERSION=""
RELEASE_ARCH=""
TMP_DIR=""
CURRENT_STEP=0
TOTAL_STEPS=5

print_help() {
  cat <<'EOF'
oore.build installer (macOS)

Usage:
  ./scripts/install.sh
  ./scripts/install.sh --help

Environment overrides:
  OORE_VERSION               Release tag or "latest" (default: latest)
  OORE_INSTALL_ROOT          Install root (default: ~/.oore)
  OORE_NONINTERACTIVE        Non-interactive mode (true/false)
  OORE_START_DAEMON          Start daemon in non-interactive mode (true/false)
  OORE_LOCAL_WEB_MODE        Local web behavior in non-interactive mode: off|run|login
  OORE_LOCAL_WEB_LISTEN      Local web listen address (default: 127.0.0.1:4173)
  OORE_HOSTED_UI             Hosted UI URL (default: https://ci.oore.build)
  OORE_GITHUB_REPO           GitHub repo (default: devaryakjha/oore.build)
  OORE_RELEASE_BASE_URL      Release asset base URL (default: GitHub Releases download base)
  OORE_RELEASE_MANIFEST_URL  Release metadata URL for latest tag resolution (default: GitHub Releases API)
EOF
}

step() {
  CURRENT_STEP=$((CURRENT_STEP + 1))
  printf '[%d/%d] %-28s' "$CURRENT_STEP" "$TOTAL_STEPS" "$1"
}

step_done() {
  printf '%s\n' "$1"
}

log() {
  printf '[oore-install] %s\n' "$*"
}

die() {
  printf '[oore-install] ERROR: %s\n' "$*" >&2
  exit 1
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
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

is_noninteractive() {
  normalize_bool "$OORE_NONINTERACTIVE"
}

prompt_yes_no() {
  local question="$1"
  local default="${2:-y}"
  local prompt=""
  local answer=""

  if [[ "$default" == "y" ]]; then
    prompt='[Y/n]'
  else
    prompt='[y/N]'
  fi

  if is_noninteractive || [[ ! -r /dev/tty ]]; then
    [[ "$default" == "y" ]]
    return
  fi

  while true; do
    printf '%s %s ' "$question" "$prompt" > /dev/tty
    if ! read -r answer < /dev/tty; then
      [[ "$default" == "y" ]]
      return
    fi

    case "$answer" in
      [Yy]|[Yy][Ee][Ss])
        return 0
        ;;
      [Nn]|[Nn][Oo])
        return 1
        ;;
      "")
        [[ "$default" == "y" ]]
        return
        ;;
      *)
        printf 'Please answer yes or no.\n' > /dev/tty
        ;;
    esac
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

resolve_release_tag() {
  local tag=""
  if [[ "$OORE_VERSION" == "latest" ]]; then
    local manifest_file="$TMP_DIR/latest.json"
    curl -fsSL --retry 3 --output "$manifest_file" "$OORE_RELEASE_MANIFEST_URL" \
      || die "Unable to fetch release manifest: $OORE_RELEASE_MANIFEST_URL"

    # GitHub API returns "tag_name": "vX.Y.Z"
    tag="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest_file" | head -n1)"
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
}

download_release_assets() {
  local archive_name="oore_${RELEASE_VERSION}_darwin_${RELEASE_ARCH}.tar.gz"
  local checksum_name="oore_${RELEASE_VERSION}_checksums.txt"
  local base_url="${OORE_RELEASE_BASE_URL%/}/$RELEASE_TAG"
  local archive_url="$base_url/$archive_name"
  local checksum_url="$base_url/$checksum_name"

  log "Downloading release assets for $RELEASE_TAG ($RELEASE_ARCH)..."
  curl -fsSL --retry 3 --output "$TMP_DIR/$archive_name" "$archive_url" \
    || die "Failed to download release archive: $archive_url"
  curl -fsSL --retry 3 --output "$TMP_DIR/$checksum_name" "$checksum_url" \
    || die "Failed to download checksum file: $checksum_url"
}

verify_archive_checksum() {
  local archive_name="oore_${RELEASE_VERSION}_darwin_${RELEASE_ARCH}.tar.gz"
  local checksum_name="oore_${RELEASE_VERSION}_checksums.txt"
  local expected=""
  local actual=""

  expected="$(
    awk -v file="$archive_name" '$2 == file { print $1 }' "$TMP_DIR/$checksum_name"
  )"
  [[ -n "$expected" ]] || die "Checksum entry for $archive_name not found in $checksum_name."

  actual="$(shasum -a 256 "$TMP_DIR/$archive_name" | awk '{ print $1 }')"
  [[ -n "$actual" ]] || die "Failed to compute checksum for $archive_name."

  if [[ "$actual" != "$expected" ]]; then
    die "Checksum mismatch for $archive_name (expected $expected, got $actual)."
  fi

  log "Checksum verified for $archive_name."
}

install_binaries() {
  local archive_name="oore_${RELEASE_VERSION}_darwin_${RELEASE_ARCH}.tar.gz"
  local extract_dir="$TMP_DIR/extract"

  mkdir -p "$extract_dir"
  tar -xzf "$TMP_DIR/$archive_name" -C "$extract_dir"

  [[ -f "$extract_dir/bin/oored" ]] || die "Release archive is missing bin/oored."
  [[ -f "$extract_dir/bin/oore" ]] || die "Release archive is missing bin/oore."
  [[ -f "$extract_dir/VERSION" ]] || die "Release archive is missing VERSION."

  mkdir -p "$BIN_DIR" "$LOG_DIR"
  cp "$extract_dir/bin/oored" "$BIN_DIR/oored"
  cp "$extract_dir/bin/oore" "$BIN_DIR/oore"
  chmod +x "$BIN_DIR/oored" "$BIN_DIR/oore"

  if [[ -f "$extract_dir/bin/oore-web" ]]; then
    cp "$extract_dir/bin/oore-web" "$WEB_BINARY"
    chmod +x "$WEB_BINARY"
  fi

  if [[ -d "$extract_dir/web-dist" ]]; then
    rm -rf "$WEB_DIST_DIR"
    cp -R "$extract_dir/web-dist" "$WEB_DIST_DIR"
  fi

  cp "$extract_dir/VERSION" "$OORE_INSTALL_ROOT/VERSION"
  if [[ -f "$extract_dir/LICENSE" ]]; then
    cp "$extract_dir/LICENSE" "$OORE_INSTALL_ROOT/LICENSE"
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
    printf '\n# oore.build\n%s\n' "$path_line" >> "$shell_rc"
    export PATH="$BIN_DIR:$PATH"
    log "Added $BIN_DIR to PATH in $shell_rc"
    log "Run 'source $shell_rc' or open a new terminal to use 'oore' and 'oored' directly."
  fi
}

start_daemon() {
  mkdir -p "$LOG_DIR"

  if curl -fsS http://127.0.0.1:8787/healthz >/dev/null 2>&1; then
    log 'A healthy daemon is already running on http://127.0.0.1:8787.'
    return 0
  fi

  log 'Starting oored in background on 127.0.0.1:8787...'
  nohup "$BIN_DIR/oored" run --listen 127.0.0.1:8787 >"$DAEMON_LOG" 2>&1 &
  echo "$!" > "$DAEMON_PID_FILE"

  local i
  for i in $(seq 1 15); do
    if curl -fsS http://127.0.0.1:8787/healthz >/dev/null 2>&1; then
      log 'Daemon is healthy.'
      return 0
    fi
    sleep 1
  done

  log "Daemon failed to become healthy. Check logs: $DAEMON_LOG"
  return 1
}

is_already_configured() {
  local status_json
  status_json="$(curl -fsS "$DAEMON_URL/v1/public/setup-status" 2>/dev/null)" || return 1
  # Check if is_configured is true in the JSON response
  echo "$status_json" | grep -q '"is_configured"[[:space:]]*:[[:space:]]*true'
}

generate_setup_token() {
  if ! curl -fsS "$DAEMON_URL/healthz" >/dev/null 2>&1; then
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
  curl -fsS "${LOCAL_WEB_URL}/__oore_web_healthz" >/dev/null 2>&1
}

start_local_web() {
  if ! has_local_web_bundle; then
    log "Bundled local web UI not found in this release."
    return 1
  fi

  mkdir -p "$LOG_DIR"

  if is_local_web_healthy; then
    log "Local web UI is already running at $LOCAL_WEB_URL."
    return 0
  fi

  nohup "$WEB_BINARY" \
    --listen "$OORE_LOCAL_WEB_LISTEN" \
    --backend-url "$DAEMON_URL" \
    --dist-dir "$WEB_DIST_DIR" >"$WEB_LOG" 2>&1 &
  echo "$!" > "$WEB_PID_FILE"

  local i
  for i in $(seq 1 15); do
    if is_local_web_healthy; then
      log "Local web UI is healthy at $LOCAL_WEB_URL."
      return 0
    fi
    sleep 1
  done

  log "Local web UI failed to become healthy. Check logs: $WEB_LOG"
  return 1
}

install_local_web_launch_agent() {
  if ! has_local_web_bundle; then
    log "Cannot install launch agent: bundled local web UI is unavailable."
    return 1
  fi

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
      <string>$DAEMON_URL</string>
      <string>--dist-dir</string>
      <string>$WEB_DIST_DIR</string>
    </array>
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
    launchctl load -w "$WEB_LAUNCH_AGENT_PLIST" >/dev/null 2>&1 \
      || return 1
  fi

  launchctl kickstart -k "gui/$uid/$WEB_LAUNCH_AGENT_LABEL" >/dev/null 2>&1 \
    || true

  log "Installed launch-at-login local web UI agent: $WEB_LAUNCH_AGENT_LABEL"
  return 0
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
      install_local_web_launch_agent \
        || die "Failed to install local web launch agent in non-interactive mode."
      start_local_web || true
      ;;
    *)
      die 'OORE_LOCAL_WEB_MODE must be one of: off,run,login.'
      ;;
  esac
}

handle_local_backend_onboarding() {
  printf '\n'
  log "Backend is running locally at $DAEMON_URL."
  log "Recommended first-run path is local-only setup."
  log ""
  log "Local-first options:"
  if has_local_web_bundle; then
    log "  1. Local web UI: $LOCAL_WEB_URL/setup"
    log "     Add instance and leave Backend URL empty (uses local proxy)."
  else
    log "  1. Local web UI: not bundled in this release build."
  fi
  log "  2. CLI setup:    $BIN_DIR/oore setup"
  log ""
  log "Remote mode (optional later):"
  log "  - Expose backend over HTTPS and open: ${OORE_HOSTED_UI}/setup?backend=<https-url>"
  log "  - Example tunnel command: cloudflared tunnel --url $DAEMON_URL"

  if has_local_web_bundle && prompt_yes_no "Start local web UI now at $LOCAL_WEB_URL?" 'y'; then
    start_local_web || true
    if have_cmd open && prompt_yes_no "Open local setup UI in browser now?" 'y'; then
      open "${LOCAL_WEB_URL}/setup" >/dev/null 2>&1 || true
    fi
    if prompt_yes_no "Auto-start local web UI at login with launchd?" 'n'; then
      install_local_web_launch_agent || log "Failed to install launch agent."
    fi
    return 0
  fi

  if prompt_yes_no "Show hosted UI setup URL now (remote mode path)?" 'n'; then
    if have_cmd open; then
      open "${OORE_HOSTED_UI}/setup" >/dev/null 2>&1 || true
    fi
  fi
}

open_setup_ui() {
  if is_localhost_backend && is_local_web_healthy; then
    if ! have_cmd open; then
      log 'Cannot auto-open browser because the `open` command is unavailable.'
      return 1
    fi
    open "${LOCAL_WEB_URL}/setup" >/dev/null 2>&1 || true
    return 0
  fi

  # The hosted UI (HTTPS) cannot make requests to a local HTTP backend
  # due to browser mixed-content restrictions. Skip auto-open.
  if is_localhost_backend; then
    return 1
  fi

  if ! have_cmd open; then
    log 'Cannot auto-open browser because the `open` command is unavailable.'
    return 1
  fi
  local setup_url="${OORE_HOSTED_UI}/setup?backend=${DAEMON_URL}"
  open "$setup_url" >/dev/null 2>&1 || true
  return 0
}

watch_setup() {
  printf '\nWaiting for setup to complete in the browser...\n'

  local prev_state=""
  local current_state=""
  local status_json=""

  while true; do
    status_json="$(curl -fsS "$DAEMON_URL/v1/public/setup-status" 2>/dev/null)" || {
      sleep 5
      continue
    }

    current_state="$(echo "$status_json" | sed -n 's/.*"state"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"

    if [[ "$current_state" != "$prev_state" ]]; then
      printf '  Current state: %s\n' "$current_state"
      prev_state="$current_state"
    fi

    # Check if setup is complete
    if echo "$status_json" | grep -q '"is_configured"[[:space:]]*:[[:space:]]*true'; then
      local instance_id
      instance_id="$(echo "$status_json" | sed -n 's/.*"instance_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
      printf '\n'
      printf 'Setup complete! Instance ID: %s\n' "$instance_id"
      printf 'Your oore.build instance is ready.\n\n'
      printf '  Dashboard:  %s\n' "$OORE_HOSTED_UI"
      printf '  API:        %s\n' "$DAEMON_URL"
      printf '  Docs:       https://docs.oore.build\n\n'
      return 0
    fi

    sleep 5
  done
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

print_next_steps() {
  local daemon_running=false
  local local_web_running=false
  if curl -fsS "$DAEMON_URL/healthz" >/dev/null 2>&1; then
    daemon_running=true
  fi
  if [[ -n "$LOCAL_WEB_URL" ]] && is_local_web_healthy; then
    local_web_running=true
  fi

  printf '\nInstallation complete.\n\n'

  if "$daemon_running"; then
    printf 'Daemon is running at %s\n\n' "$DAEMON_URL"
    printf 'Complete setup (local-first):\n'
    if has_local_web_bundle; then
      printf '  %s/setup\n' "$LOCAL_WEB_URL"
      printf '  (or use CLI below)\n'
    fi
    printf '  oore setup                    # interactive CLI setup\n'
    printf '  oore setup token --ttl 15m     # generate a new bootstrap token\n'
    if has_local_web_bundle && "$local_web_running"; then
      printf '  local web status: running\n'
    elif has_local_web_bundle; then
      printf '  local web start:  oore-web --backend-url %s\n' "$DAEMON_URL"
    fi
    printf '\nRemote mode (optional later, requires HTTPS backend):\n'
    printf '  %s\n' "$OORE_HOSTED_UI"
  else
    printf 'Start the daemon:\n'
    printf '  oored run --listen 127.0.0.1:8787\n\n'
    printf 'Then complete setup (local-first):\n'
    if has_local_web_bundle; then
      printf '  %s/setup\n' "$LOCAL_WEB_URL"
      printf '  (or use CLI below)\n'
    fi
    printf '  oore setup                    # interactive CLI setup\n'
    printf '  oore setup token --ttl 15m     # generate a bootstrap token\n'
    if has_local_web_bundle; then
      printf '  local web start:  oore-web --backend-url %s\n' "$DAEMON_URL"
    fi
    printf '\nRemote mode (optional later, requires HTTPS backend):\n'
    printf '  %s\n' "$OORE_HOSTED_UI"
  fi

  printf '\nDocs: https://docs.oore.build\n'
}

cleanup() {
  if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR"
  fi
}

main() {
  if [[ $# -gt 0 ]]; then
    case "$1" in
      -h|--help)
        print_help
        return 0
        ;;
      *)
        die "Unknown argument: $1 (use --help)"
        ;;
    esac
  fi

  trap cleanup EXIT

  validate_local_web_mode

  if normalize_bool "$OORE_NONINTERACTIVE"; then
    :
  else
    if [[ "$?" -eq 2 ]]; then
      die 'OORE_NONINTERACTIVE must be one of: 1,0,true,false,yes,no,on,off.'
    fi
  fi

  if [[ "$(uname -s)" != "Darwin" ]]; then
    die 'oore.build V1 backend installer currently supports macOS only.'
  fi

  ensure_dependency curl
  ensure_dependency tar
  ensure_dependency shasum
  ensure_dependency awk
  ensure_dependency uname
  ensure_dependency mktemp

  ensure_install_root_writable

  # Step 1: Detect platform
  step "Detecting platform..."
  detect_arch
  step_done "macOS $RELEASE_ARCH"

  TMP_DIR="$(mktemp -d)"
  resolve_local_web_url

  # Step 2: Download
  resolve_release_tag
  step "Downloading $RELEASE_TAG..."
  download_release_assets
  step_done "oore_${RELEASE_VERSION}_darwin_${RELEASE_ARCH}.tar.gz"

  # Step 3: Verify checksum
  step "Verifying checksum..."
  verify_archive_checksum
  step_done "SHA-256 verified"

  # Step 4: Install binaries
  step "Installing binaries..."
  install_binaries
  if has_local_web_bundle; then
    step_done "$BIN_DIR/{oored,oore,oore-web}"
  else
    step_done "$BIN_DIR/{oored,oore}"
  fi

  ensure_on_path

  if is_noninteractive; then
    # Step 5: Non-interactive daemon handling
    if [[ -n "$OORE_START_DAEMON" ]]; then
      if normalize_bool "$OORE_START_DAEMON"; then
        step "Starting daemon..."
        start_daemon || die "Daemon startup failed. Check logs: $DAEMON_LOG"
        if is_localhost_backend; then
          configure_local_web_noninteractive
        fi
        step_done "$DAEMON_URL (healthy)"
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
    # Step 5: Interactive — auto-start daemon
    step "Starting daemon..."
    if start_daemon; then
      step_done "$DAEMON_URL (healthy)"

      # Auto-generate bootstrap token if not already configured
      if ! is_already_configured; then
        printf '\n'
        generate_setup_token || true

        if is_localhost_backend; then
          handle_local_backend_onboarding
        else
          printf '\nPress Ctrl+C to exit (setup can continue in the browser).\n'

          # Open hosted UI with pre-filled backend URL
          open_setup_ui || true

          # Watch for setup completion
          watch_setup || true
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
}

main "$@"
