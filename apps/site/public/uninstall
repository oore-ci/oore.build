#!/usr/bin/env bash
set -euo pipefail

OORE_INSTALL_ROOT="${OORE_INSTALL_ROOT:-$HOME/.oore}"
OORE_NONINTERACTIVE="${OORE_NONINTERACTIVE:-0}"

BIN_DIR="$OORE_INSTALL_ROOT/bin"
DAEMON_PID_FILE="$OORE_INSTALL_ROOT/oored.pid"
DATA_DIR="$HOME/Library/Application Support/oore"

log() {
  printf '[oore-uninstall] %s\n' "$*"
}

die() {
  printf '[oore-uninstall] ERROR: %s\n' "$*" >&2
  exit 1
}

normalize_bool() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON) return 0 ;;
    0|false|FALSE|no|NO|off|OFF) return 1 ;;
    *) return 2 ;;
  esac
}

is_noninteractive() {
  normalize_bool "$OORE_NONINTERACTIVE"
}

prompt_yes_no() {
  local question="$1"
  local default="${2:-n}"
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
      [Yy]|[Yy][Ee][Ss]) return 0 ;;
      [Nn]|[Nn][Oo]) return 1 ;;
      "") [[ "$default" == "y" ]]; return ;;
      *) printf 'Please answer yes or no.\n' > /dev/tty ;;
    esac
  done
}

stop_daemon() {
  # Stop via PID file
  if [[ -f "$DAEMON_PID_FILE" ]]; then
    local pid=""
    pid="$(cat "$DAEMON_PID_FILE" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      log "Stopping daemon (PID $pid)..."
      kill "$pid" >/dev/null 2>&1 || true
      sleep 1
    fi
    rm -f "$DAEMON_PID_FILE"
  fi

  # Also check if anything is still listening on the default port
  if command -v lsof >/dev/null 2>&1; then
    local pids=""
    pids="$(lsof -nP -iTCP:8787 -sTCP:LISTEN -t 2>/dev/null || true)"
    if [[ -n "$pids" ]]; then
      log "Stopping processes on port 8787..."
      echo "$pids" | xargs kill 2>/dev/null || true
      sleep 1
    fi
  fi
}

remove_from_path() {
  local rc_files=("$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile")

  for rc in "${rc_files[@]}"; do
    if [[ ! -f "$rc" ]]; then
      continue
    fi

    if grep -qF "$BIN_DIR" "$rc" 2>/dev/null; then
      log "Removing PATH entry from $rc..."
      # Remove the "# oore.build" comment line and the export PATH line
      local tmp="$rc.oore-uninstall.tmp"
      grep -v "# oore.build" "$rc" | grep -vF "$BIN_DIR" > "$tmp" || true
      mv "$tmp" "$rc"
    fi
  done
}

remove_install_dir() {
  if [[ ! -d "$OORE_INSTALL_ROOT" ]]; then
    log "Install directory not found: $OORE_INSTALL_ROOT (already removed?)"
    return 0
  fi

  log "Removing install directory: $OORE_INSTALL_ROOT"
  rm -rf "$OORE_INSTALL_ROOT"
}

remove_data_dir() {
  if [[ ! -d "$DATA_DIR" ]]; then
    return 0
  fi

  if is_noninteractive; then
    log "Removing data directory: $DATA_DIR"
    rm -rf "$DATA_DIR"
  elif prompt_yes_no "Remove application data ($DATA_DIR)? This includes your database." 'n'; then
    log "Removing data directory: $DATA_DIR"
    rm -rf "$DATA_DIR"
  else
    log "Keeping data directory: $DATA_DIR"
  fi
}

main() {
  if [[ ! -d "$OORE_INSTALL_ROOT" ]] && ! grep -rqF "$BIN_DIR" "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile" 2>/dev/null; then
    log "oore.build does not appear to be installed."
    exit 0
  fi

  log "This will uninstall oore.build from your system."
  log ""
  log "  Install dir: $OORE_INSTALL_ROOT"
  log "  Data dir:    $DATA_DIR"
  log ""

  if ! is_noninteractive; then
    if ! prompt_yes_no "Continue with uninstall?" 'n'; then
      log "Uninstall cancelled."
      exit 0
    fi
  fi

  stop_daemon
  remove_from_path
  remove_install_dir
  remove_data_dir

  log ""
  log "oore.build has been uninstalled."
  log "Open a new terminal for PATH changes to take effect."
}

main "$@"
