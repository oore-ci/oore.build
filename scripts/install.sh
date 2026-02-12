#!/usr/bin/env bash
set -euo pipefail

OORE_VERSION="${OORE_VERSION:-latest}"
OORE_INSTALL_ROOT="${OORE_INSTALL_ROOT:-$HOME/.oore}"
OORE_RELEASE_BASE_URL="${OORE_RELEASE_BASE_URL:-https://dl.oore.build/releases}"
OORE_RELEASE_MANIFEST_URL="${OORE_RELEASE_MANIFEST_URL:-$OORE_RELEASE_BASE_URL/latest.json}"
OORE_NONINTERACTIVE="${OORE_NONINTERACTIVE:-0}"
OORE_START_DAEMON="${OORE_START_DAEMON:-}"
OORE_HOSTED_UI="${OORE_HOSTED_UI:-https://ci.oore.build}"

BIN_DIR="$OORE_INSTALL_ROOT/bin"
LOG_DIR="$OORE_INSTALL_ROOT/logs"
DAEMON_LOG="$LOG_DIR/oored.log"
DAEMON_PID_FILE="$OORE_INSTALL_ROOT/oored.pid"
DAEMON_URL="http://127.0.0.1:8787"
RELEASE_TAG=""
RELEASE_VERSION=""
RELEASE_ARCH=""
TMP_DIR=""
CURRENT_STEP=0
TOTAL_STEPS=5

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

    tag="$(sed -n 's/.*"tag"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest_file" | head -n1)"
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

  cp "$extract_dir/VERSION" "$OORE_INSTALL_ROOT/VERSION"
  if [[ -f "$extract_dir/LICENSE" ]]; then
    cp "$extract_dir/LICENSE" "$OORE_INSTALL_ROOT/LICENSE"
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

  "$BIN_DIR/oore" setup open --ttl 15m \
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

open_setup_ui() {
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
  cat <<DONE

Installation complete.

Next steps:
  1) Ensure daemon is running:
     $BIN_DIR/oored run --listen 127.0.0.1:8787

  2) Generate a bootstrap setup token:
     $BIN_DIR/oore setup open --ttl 15m

  3) Complete setup via CLI or hosted UI:
     CLI:        $BIN_DIR/oore setup
     Hosted UI:  $OORE_HOSTED_UI (requires HTTPS-reachable backend)

  4) Read setup docs:
     https://docs.oore.build

Environment variables:
  OORE_VERSION=latest             # 'latest' or a tag like v0.2.0
  OORE_INSTALL_ROOT=~/.oore       # Installation directory
  OORE_RELEASE_BASE_URL=https://dl.oore.build/releases
                                  # Release base URL (contains /<tag>/assets)
  OORE_RELEASE_MANIFEST_URL=https://dl.oore.build/releases/latest.json
                                  # Manifest URL used when OORE_VERSION=latest
  OORE_NONINTERACTIVE=1           # Disable prompts
  OORE_START_DAEMON=true|false    # Non-interactive daemon startup behavior

DONE
}

cleanup() {
  if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR"
  fi
}

main() {
  trap cleanup EXIT

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

  # Step 1: Detect platform
  step "Detecting platform..."
  detect_arch
  step_done "macOS $RELEASE_ARCH"

  TMP_DIR="$(mktemp -d)"

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
  step_done "$BIN_DIR/{oored,oore}"

  if is_noninteractive; then
    # Step 5: Non-interactive daemon handling
    if [[ -n "$OORE_START_DAEMON" ]]; then
      if normalize_bool "$OORE_START_DAEMON"; then
        step "Starting daemon..."
        start_daemon || die "Daemon startup failed. Check logs: $DAEMON_LOG"
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
          printf '\n'
          log "Your backend is on localhost. The hosted UI (https://ci.oore.build)"
          log "cannot reach a local HTTP backend due to browser security restrictions."
          log ""
          log "To complete setup, choose one of:"
          log "  1. CLI setup:    $BIN_DIR/oore setup"
          log "  2. Tunnel:       Expose your backend via a tunnel (e.g. cloudflared)"
          log "                   then open ${OORE_HOSTED_UI}/setup?backend=<tunnel-url>"
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
