#!/usr/bin/env bash
set -euo pipefail

OORED_DEV_DATA_DIR="${OORED_DEV_DATA_DIR:-$HOME/.oore/dev.noindex}"
OORED_DEV_LISTEN_ADDR="${OORED_DEV_LISTEN_ADDR:-127.0.0.1:8787}"
OORE_DEV_DAEMON_URL="${OORE_DEV_DAEMON_URL:-http://$OORED_DEV_LISTEN_ADDR}"
OORE_DEV_SETUP_STATE_FILE="${OORE_DEV_SETUP_STATE_FILE:-$OORED_DEV_DATA_DIR/oore.db}"
OORE_DEV_SETUP_TTL="${OORE_DEV_SETUP_TTL:-15m}"
OORE_DEV_SETUP_MODE="${OORE_DEV_SETUP_MODE:-token}"
OORE_DEV_BUILD_PROFILE="${OORE_DEV_BUILD_PROFILE:-release}"
OORE_DEV_LOG_LEVEL="${OORE_DEV_LOG_LEVEL:-info}"
OORE_DEV_CLEAN="${OORE_DEV_CLEAN:-1}"
OORE_DEV_HEALTH_TIMEOUT_SECS="${OORE_DEV_HEALTH_TIMEOUT_SECS:-45}"
OORE_DEV_ENABLE_TUNNEL="${OORE_DEV_ENABLE_TUNNEL:-1}"
OORE_DEV_TUNNEL_TIMEOUT_SECS="${OORE_DEV_TUNNEL_TIMEOUT_SECS:-45}"
OORE_DEV_HOSTED_UI="${OORE_DEV_HOSTED_UI:-https://ci.oore.build}"
OORE_DEV_AUTO_OPEN="${OORE_DEV_AUTO_OPEN:-1}"
OORE_DEV_WATCH="${OORE_DEV_WATCH:-1}"

OORE_DEV_LOG_DIR="$OORED_DEV_DATA_DIR/logs"
OORE_DEV_DAEMON_LOG="$OORE_DEV_LOG_DIR/oored-dev.log"
OORE_DEV_DAEMON_PID_FILE="$OORED_DEV_DATA_DIR/oored-dev.pid"
OORE_DEV_TUNNEL_LOG="$OORE_DEV_LOG_DIR/cloudflared-dev.log"
OORE_DEV_TUNNEL_PID_FILE="$OORED_DEV_DATA_DIR/cloudflared-dev.pid"

OORED_BIN=""
OORE_BIN=""
OORE_DEV_PUBLIC_TUNNEL_URL=""
OORE_DEV_BOOTSTRAP_TOKEN=""
SPOTLIGHT_NO_INDEX_SENTINEL=".metadata_never_index"

log() {
  printf '[dev-fresh-setup] %s\n' "$*"
}

die() {
  printf '[dev-fresh-setup] ERROR: %s\n' "$*" >&2
  exit 1
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

is_true() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    *) return 1 ;;
  esac
}

validate_safe_dev_dir() {
  case "$OORED_DEV_DATA_DIR" in
    ""|"/"|"$HOME"|"$HOME/"|"$HOME/.oore"|"$HOME/.oore/")
      die "Refusing to use unsafe OORED_DEV_DATA_DIR value: $OORED_DEV_DATA_DIR"
      ;;
  esac
}

stop_previous_dev_daemon() {
  if [[ ! -f "$OORE_DEV_DAEMON_PID_FILE" ]]; then
    return 0
  fi

  local pid=""
  pid="$(cat "$OORE_DEV_DAEMON_PID_FILE" 2>/dev/null || true)"
  rm -f "$OORE_DEV_DAEMON_PID_FILE"

  if [[ -z "$pid" ]]; then
    return 0
  fi

  if kill -0 "$pid" >/dev/null 2>&1; then
    log "Stopping previous dev daemon PID $pid..."
    kill "$pid" >/dev/null 2>&1 || true
    sleep 1
  fi
}

stop_previous_tunnel() {
  if [[ ! -f "$OORE_DEV_TUNNEL_PID_FILE" ]]; then
    return 0
  fi

  local pid=""
  pid="$(cat "$OORE_DEV_TUNNEL_PID_FILE" 2>/dev/null || true)"
  rm -f "$OORE_DEV_TUNNEL_PID_FILE"

  if [[ -z "$pid" ]]; then
    return 0
  fi

  if kill -0 "$pid" >/dev/null 2>&1; then
    log "Stopping previous Cloudflare tunnel PID $pid..."
    kill "$pid" >/dev/null 2>&1 || true
    sleep 1
  fi
}

assert_listen_port_free() {
  local port=""
  if [[ "$OORED_DEV_LISTEN_ADDR" != *:* ]]; then
    die "OORED_DEV_LISTEN_ADDR must be in host:port format (got: $OORED_DEV_LISTEN_ADDR)"
  fi
  port="${OORED_DEV_LISTEN_ADDR##*:}"
  [[ "$port" =~ ^[0-9]+$ ]] || die "Invalid listen port in OORED_DEV_LISTEN_ADDR: $OORED_DEV_LISTEN_ADDR"

  if curl -fsS "$OORE_DEV_DAEMON_URL/healthz" >/dev/null 2>&1; then
    die "A daemon is already healthy at $OORE_DEV_DAEMON_URL. Stop it first or change OORED_DEV_LISTEN_ADDR/OORE_DEV_DAEMON_URL."
  fi

  if have_cmd lsof && lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    die "Port $port is already in use. Stop the existing process or choose another OORED_DEV_LISTEN_ADDR."
  fi
}

clean_dev_state() {
  if [[ "$OORE_DEV_CLEAN" == "0" ]]; then
    log "Skipping dev state cleanup (OORE_DEV_CLEAN=0)."
    return 0
  fi

  validate_safe_dev_dir
  log "Removing dev state at $OORED_DEV_DATA_DIR..."
  rm -rf "$OORED_DEV_DATA_DIR"
}

mark_no_spotlight_index() {
  mkdir -p "$OORED_DEV_DATA_DIR"
  local sentinel="$OORED_DEV_DATA_DIR/$SPOTLIGHT_NO_INDEX_SENTINEL"
  if [[ -f "$sentinel" ]]; then
    return 0
  fi
  : > "$sentinel" || die "Failed to write Spotlight no-index marker: $sentinel"
}

build_local_binaries() {
  log "Building local binaries (profile: $OORE_DEV_BUILD_PROFILE)..."
  case "$OORE_DEV_BUILD_PROFILE" in
    release)
      cargo build --release -p oored -p oore
      OORED_BIN="target/release/oored"
      OORE_BIN="target/release/oore"
      ;;
    debug)
      cargo build -p oored -p oore
      OORED_BIN="target/debug/oored"
      OORE_BIN="target/debug/oore"
      ;;
    *)
      die "Unsupported OORE_DEV_BUILD_PROFILE: $OORE_DEV_BUILD_PROFILE (expected: release|debug)"
      ;;
  esac

  [[ -x "$OORED_BIN" ]] || die "Missing oored binary: $OORED_BIN"
  [[ -x "$OORE_BIN" ]] || die "Missing oore binary: $OORE_BIN"
}

start_dev_daemon() {
  mkdir -p "$OORE_DEV_LOG_DIR"

  local cors_origins="http://localhost:3000,https://ci.oore.build"
  if [[ -n "$OORE_DEV_PUBLIC_TUNNEL_URL" ]]; then
    cors_origins="${cors_origins},${OORE_DEV_PUBLIC_TUNNEL_URL}"
    log "CORS origins include tunnel: $OORE_DEV_PUBLIC_TUNNEL_URL"
  fi

  log "Starting dev daemon on $OORED_DEV_LISTEN_ADDR..."
  OORED_DATA_DIR="$OORED_DEV_DATA_DIR" \
  OORE_SETUP_STATE_FILE="$OORE_DEV_SETUP_STATE_FILE" \
  OORE_CORS_ORIGINS="$cors_origins" \
  RUST_LOG="$OORE_DEV_LOG_LEVEL" \
  "$OORED_BIN" run --listen "$OORED_DEV_LISTEN_ADDR" >"$OORE_DEV_DAEMON_LOG" 2>&1 &
  echo "$!" > "$OORE_DEV_DAEMON_PID_FILE"

  local waited=0
  until curl -fsS "$OORE_DEV_DAEMON_URL/healthz" >/dev/null 2>&1; do
    waited=$((waited + 1))
    if [[ "$waited" -ge "$OORE_DEV_HEALTH_TIMEOUT_SECS" ]]; then
      tail -n 80 "$OORE_DEV_DAEMON_LOG" >&2 || true
      die "Daemon failed health check at $OORE_DEV_DAEMON_URL/healthz"
    fi
    sleep 1
  done

  log "Daemon is healthy."
}

start_cloudflare_tunnel() {
  if ! is_true "$OORE_DEV_ENABLE_TUNNEL"; then
    log "Skipping Cloudflare tunnel (OORE_DEV_ENABLE_TUNNEL=$OORE_DEV_ENABLE_TUNNEL)."
    return 0
  fi

  have_cmd cloudflared || die "cloudflared is required for tunnel mode. Install it or set OORE_DEV_ENABLE_TUNNEL=0."

  mkdir -p "$OORE_DEV_LOG_DIR"
  log "Starting Cloudflare quick tunnel for $OORE_DEV_DAEMON_URL..."
  cloudflared tunnel --url "$OORE_DEV_DAEMON_URL" --no-autoupdate >"$OORE_DEV_TUNNEL_LOG" 2>&1 &
  echo "$!" > "$OORE_DEV_TUNNEL_PID_FILE"

  local waited=0
  local url=""

  while true; do
    if [[ -f "$OORE_DEV_TUNNEL_LOG" ]]; then
      url="$(grep -Eo 'https://[-a-zA-Z0-9]+\\.trycloudflare\\.com' "$OORE_DEV_TUNNEL_LOG" | head -n1 || true)"
      if [[ -n "$url" ]]; then
        OORE_DEV_PUBLIC_TUNNEL_URL="$url"
        log "Cloudflare tunnel URL: $OORE_DEV_PUBLIC_TUNNEL_URL"
        return 0
      fi
    fi

    local pid=""
    pid="$(cat "$OORE_DEV_TUNNEL_PID_FILE" 2>/dev/null || true)"
    if [[ -z "$pid" ]] || ! kill -0 "$pid" >/dev/null 2>&1; then
      tail -n 80 "$OORE_DEV_TUNNEL_LOG" >&2 || true
      die "Cloudflare tunnel failed to start."
    fi

    waited=$((waited + 1))
    if [[ "$waited" -ge "$OORE_DEV_TUNNEL_TIMEOUT_SECS" ]]; then
      tail -n 80 "$OORE_DEV_TUNNEL_LOG" >&2 || true
      die "Timed out waiting for Cloudflare tunnel URL."
    fi
    sleep 1
  done
}

open_setup_ui() {
  if ! is_true "$OORE_DEV_AUTO_OPEN"; then
    return 0
  fi
  if ! have_cmd open; then
    log "Cannot auto-open browser (open command unavailable)."
    return 1
  fi

  local backend_url="$OORE_DEV_DAEMON_URL"
  if [[ -n "$OORE_DEV_PUBLIC_TUNNEL_URL" ]]; then
    backend_url="$OORE_DEV_PUBLIC_TUNNEL_URL"
  fi

  local setup_url="${OORE_DEV_HOSTED_UI}/setup?backend=${backend_url}"
  log "Opening: $setup_url"
  open "$setup_url" >/dev/null 2>&1 || true
}

watch_setup() {
  if ! is_true "$OORE_DEV_WATCH"; then
    return 0
  fi

  printf '\nWaiting for setup to complete in the browser...\n'
  printf 'Press Ctrl+C to exit (setup can continue in the browser).\n\n'

  local prev_state=""
  local current_state=""
  local status_json=""

  while true; do
    status_json="$(curl -fsS "$OORE_DEV_DAEMON_URL/v1/public/setup-status" 2>/dev/null)" || {
      sleep 5
      continue
    }

    current_state="$(echo "$status_json" | sed -n 's/.*"state"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"

    if [[ "$current_state" != "$prev_state" ]]; then
      printf '  Current state: %s\n' "$current_state"
      prev_state="$current_state"
    fi

    if echo "$status_json" | grep -q '"is_configured"[[:space:]]*:[[:space:]]*true'; then
      local instance_id
      instance_id="$(echo "$status_json" | sed -n 's/.*"instance_id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
      printf '\nSetup complete! Instance ID: %s\n' "$instance_id"
      printf 'Your Oore dev instance is ready.\n\n'
      local backend_url="$OORE_DEV_DAEMON_URL"
      if [[ -n "$OORE_DEV_PUBLIC_TUNNEL_URL" ]]; then
        backend_url="$OORE_DEV_PUBLIC_TUNNEL_URL"
      fi
      printf '  Dashboard:  %s\n' "$OORE_DEV_HOSTED_UI"
      printf '  API:        %s\n' "$backend_url"
      printf '  Docs:       https://docs.oore.build\n\n'
      return 0
    fi

    sleep 5
  done
}

run_setup_flow() {
  case "$OORE_DEV_SETUP_MODE" in
    cli)
      log "Running full interactive setup via CLI..."
      OORED_DATA_DIR="$OORED_DEV_DATA_DIR" \
      OORE_SETUP_STATE_FILE="$OORE_DEV_SETUP_STATE_FILE" \
      OORE_DAEMON_URL="$OORE_DEV_DAEMON_URL" \
      "$OORE_BIN" setup --daemon-url "$OORE_DEV_DAEMON_URL"
      ;;
    token)
      log "Generating bootstrap token only..."
      local token_json=""
      token_json="$(
        OORED_DATA_DIR="$OORED_DEV_DATA_DIR" \
        OORE_SETUP_STATE_FILE="$OORE_DEV_SETUP_STATE_FILE" \
        "$OORE_BIN" setup token --ttl "$OORE_DEV_SETUP_TTL" --json
      )"
      printf '%s\n' "$token_json"
      OORE_DEV_BOOTSTRAP_TOKEN="$(printf '%s\n' "$token_json" | sed -n 's/.*"token"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
      if [[ -z "$OORE_DEV_BOOTSTRAP_TOKEN" ]]; then
        die "Failed to parse bootstrap token from setup token output."
      fi
      ;;
    none)
      log "Skipping setup flow (OORE_DEV_SETUP_MODE=none)."
      ;;
    *)
      die "Unsupported OORE_DEV_SETUP_MODE: $OORE_DEV_SETUP_MODE (expected: cli|token|none)"
      ;;
  esac
}

print_summary() {
  log "Done."
  log "Data dir:  $OORED_DEV_DATA_DIR"
  log "DB file:   $OORE_DEV_SETUP_STATE_FILE"
  log "Daemon:    $OORE_DEV_DAEMON_URL"
  log "Logs:      $OORE_DEV_DAEMON_LOG"
  log "PID file:  $OORE_DEV_DAEMON_PID_FILE"
  if [[ -n "$OORE_DEV_PUBLIC_TUNNEL_URL" ]]; then
    log "Public URL: $OORE_DEV_PUBLIC_TUNNEL_URL"
    log "Tunnel log: $OORE_DEV_TUNNEL_LOG"
    log "Tunnel PID: $OORE_DEV_TUNNEL_PID_FILE"
  fi
  if [[ -n "$OORE_DEV_BOOTSTRAP_TOKEN" ]]; then
    log "Bootstrap token: $OORE_DEV_BOOTSTRAP_TOKEN"
    local backend_url="$OORE_DEV_DAEMON_URL"
    if [[ -n "$OORE_DEV_PUBLIC_TUNNEL_URL" ]]; then
      backend_url="$OORE_DEV_PUBLIC_TUNNEL_URL"
    fi
    log "Setup UI: ${OORE_DEV_HOSTED_UI}/setup?backend=${backend_url}"
    log "Backend URL: $backend_url"
  fi
  log "Status endpoint:"
  curl -fsS "$OORE_DEV_DAEMON_URL/v1/public/setup-status" || true
  printf '\n'
}

main() {
  have_cmd cargo || die "cargo is required"
  have_cmd curl || die "curl is required"

  stop_previous_tunnel
  stop_previous_dev_daemon
  assert_listen_port_free
  clean_dev_state
  mark_no_spotlight_index
  build_local_binaries
  start_cloudflare_tunnel
  start_dev_daemon
  run_setup_flow
  print_summary

  if [[ -n "$OORE_DEV_BOOTSTRAP_TOKEN" ]]; then
    open_setup_ui || true
    watch_setup || true
  fi
}

main "$@"
