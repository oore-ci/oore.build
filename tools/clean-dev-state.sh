#!/usr/bin/env bash
set -euo pipefail

OORED_DEV_DATA_DIR="${OORED_DEV_DATA_DIR:-$HOME/.oore/dev.noindex}"
OORED_DEV_LISTEN_ADDR="${OORED_DEV_LISTEN_ADDR:-127.0.0.1:8787}"
OORE_DEV_DAEMON_URL="${OORE_DEV_DAEMON_URL:-http://$OORED_DEV_LISTEN_ADDR}"

OORE_DEV_DAEMON_PID_FILE="$OORED_DEV_DATA_DIR/oored-dev.pid"
OORE_DEV_TUNNEL_PID_FILE="$OORED_DEV_DATA_DIR/cloudflared-dev.pid"

log() {
  printf '[clean-dev-state] %s\n' "$*"
}

validate_safe_dev_dir() {
  case "$OORED_DEV_DATA_DIR" in
    ""|"/"|"$HOME"|"$HOME/"|"$HOME/.oore"|"$HOME/.oore/")
      log "Refusing to delete unsafe OORED_DEV_DATA_DIR value: $OORED_DEV_DATA_DIR"
      exit 1
      ;;
  esac
}

stop_by_pid_file() {
  local pid_file="$1"
  local must_contain="$2"
  local label="$3"

  if [[ ! -f "$pid_file" ]]; then
    return 0
  fi

  local pid=""
  pid="$(cat "$pid_file" 2>/dev/null || true)"
  rm -f "$pid_file"

  if [[ -z "$pid" ]] || ! kill -0 "$pid" >/dev/null 2>&1; then
    return 0
  fi

  local cmd=""
  cmd="$(ps -p "$pid" -o command= 2>/dev/null || true)"
  if [[ "$cmd" == *"$must_contain"* ]]; then
    log "Stopping $label PID $pid"
    kill "$pid" >/dev/null 2>&1 || true
    sleep 1
  fi
}

stop_by_pattern() {
  local pattern="$1"
  local label="$2"
  local pids=""

  pids="$(pgrep -f "$pattern" 2>/dev/null || true)"
  if [[ -z "$pids" ]]; then
    return 0
  fi

  log "Stopping $label by pattern"
  while IFS= read -r pid; do
    [[ -z "$pid" ]] && continue
    kill "$pid" >/dev/null 2>&1 || true
  done <<< "$pids"
}

main() {
  validate_safe_dev_dir

  stop_by_pid_file "$OORE_DEV_DAEMON_PID_FILE" "--listen $OORED_DEV_LISTEN_ADDR" "dev daemon"
  stop_by_pid_file "$OORE_DEV_TUNNEL_PID_FILE" "cloudflared tunnel --url $OORE_DEV_DAEMON_URL" "Cloudflare tunnel"

  # Fallback: kill any stale matching processes even if PID files were missing.
  stop_by_pattern "oored run --listen $OORED_DEV_LISTEN_ADDR" "dev daemon"
  stop_by_pattern "cloudflared tunnel --url $OORE_DEV_DAEMON_URL" "Cloudflare tunnel"

  log "Removing dev state: $OORED_DEV_DATA_DIR"
  rm -rf "$OORED_DEV_DATA_DIR"
}

main "$@"

