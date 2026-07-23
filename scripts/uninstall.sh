#!/usr/bin/env bash
set -euo pipefail

OORE_INSTALL_ROOT="${OORE_INSTALL_ROOT:-$HOME/.oore}"
OORE_NONINTERACTIVE="${OORE_NONINTERACTIVE:-0}"

BIN_DIR="$OORE_INSTALL_ROOT/bin"
DAEMON_PID_FILE="$OORE_INSTALL_ROOT/oored.pid"
WEB_PID_FILE="$OORE_INSTALL_ROOT/oore-web.pid"
DATA_DIR="$HOME/Library/Application Support/oore"
DAEMON_LAUNCH_AGENT_LABEL="build.oore.oored"
DAEMON_LAUNCH_AGENT_PLIST="$HOME/Library/LaunchAgents/$DAEMON_LAUNCH_AGENT_LABEL.plist"
DAEMON_LAUNCH_DAEMON_PLIST="/Library/LaunchDaemons/$DAEMON_LAUNCH_AGENT_LABEL.plist"
UPDATER_LAUNCH_DAEMON_LABEL="build.oore.oore-updater"
UPDATER_LAUNCH_DAEMON_PLIST="/Library/LaunchDaemons/$UPDATER_LAUNCH_DAEMON_LABEL.plist"
RUNNER_LAUNCH_AGENT_LABEL="build.oore.oore-runner"
RUNNER_LAUNCH_AGENT_PLIST="$HOME/Library/LaunchAgents/$RUNNER_LAUNCH_AGENT_LABEL.plist"
RUNNER_LAUNCH_DAEMON_PLIST="/Library/LaunchDaemons/$RUNNER_LAUNCH_AGENT_LABEL.plist"
WEB_LAUNCH_AGENT_LABEL="build.oore.oore-web"
WEB_LAUNCH_AGENT_PLIST="$HOME/Library/LaunchAgents/$WEB_LAUNCH_AGENT_LABEL.plist"
WEB_SYSTEMD_USER_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
WEB_SYSTEMD_SERVICE_NAME="oore-web.service"
WEB_SYSTEMD_SERVICE_FILE="$WEB_SYSTEMD_USER_DIR/$WEB_SYSTEMD_SERVICE_NAME"
UI_RESET=""
UI_BOLD=""
UI_DIM=""
UI_ACCENT=""
UI_SUCCESS=""
UI_WARNING=""
UI_ERROR=""

log() {
  printf '%b[oore-uninstall]%b %s\n' "$UI_BOLD$UI_ACCENT" "$UI_RESET" "$*"
}

die() {
  printf '%b[oore-uninstall] ERROR:%b %s\n' "$UI_BOLD$UI_ERROR" "$UI_RESET" "$*" >&2
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

print_uninstall_intro() {
  printf '\n'
  print_ascii_banner
  printf '%bOore CI Uninstaller%b\n' "$UI_BOLD$UI_ACCENT" "$UI_RESET"
  printf '%b----------------------------------------%b\n' "$UI_DIM" "$UI_RESET"
  printf '  Prompting:     %s\n' "$(ui_prompt_mode)"
  printf '  Install dir:   %s\n' "$OORE_INSTALL_ROOT"
  printf '  Data dir:      %s\n' "$DATA_DIR"
  if is_noninteractive; then
    printf '  Data action:   remove automatically\n'
  else
    printf '  Data action:   ask before removing\n'
  fi
  printf '%b----------------------------------------%b\n' "$UI_DIM" "$UI_RESET"
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
  local default="${2:-n}"
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

remove_system_launchd_service() {
  local label="$1"
  local plist="$2"
  local component="$3"

  if [[ ! -e "$plist" ]] && ! system_launchd_job_is_loaded "$label"; then
    return 0
  fi

  if ! sudo -v; then
    die "Administrator access is required to remove the $component system service. The install directory was left intact."
  fi

  sudo -n /bin/launchctl bootout "system/$label" >/dev/null 2>&1 || true
  sudo -n /bin/launchctl remove "$label" >/dev/null 2>&1 || true

  if sudo -n /bin/launchctl print "system/$label" >/dev/null 2>&1; then
    die "The $component system service is still loaded. Its service definition and the install directory were left intact."
  fi

  if ! sudo -n /bin/rm -f "$plist" >/dev/null 2>&1; then
    die "Could not remove the $component system service. The install directory was left intact."
  fi
  if [[ -e "$plist" ]]; then
    die "The $component system service definition is still installed. The install directory was left intact."
  fi
}

system_launchd_job_is_loaded() {
  local label="$1"
  command -v launchctl >/dev/null 2>&1 \
    && launchctl print "system/$label" >/dev/null 2>&1
}

remove_user_launchd_service() {
  local label="$1"
  local plist="$2"
  local component="$3"
  local uid=""

  if [[ ! -e "$plist" ]] && ! user_launchd_job_is_loaded "$label"; then
    return 0
  fi

  if command -v launchctl >/dev/null 2>&1; then
    uid="$(id -u)"
    launchctl bootout "gui/$uid/$label" >/dev/null 2>&1 || true
    launchctl remove "$label" >/dev/null 2>&1 || true
    if launchctl print "gui/$uid/$label" >/dev/null 2>&1; then
      die "The $component is still loaded. Its service definition and the install directory were left intact."
    fi
  fi

  if [[ -e "$plist" ]]; then
    log "Removing $component: $plist"
    rm -f "$plist" || die "Could not remove the $component. The install directory was left intact."
  fi
  if [[ -e "$plist" ]]; then
    die "The $component service definition is still installed. The install directory was left intact."
  fi
}

user_launchd_job_is_loaded() {
  local label="$1"
  local uid=""
  command -v launchctl >/dev/null 2>&1 || return 1
  uid="$(id -u)"
  launchctl print "gui/$uid/$label" >/dev/null 2>&1
}

remove_daemon_launch_agent() {
  remove_system_launchd_service \
    "$DAEMON_LAUNCH_AGENT_LABEL" \
    "$DAEMON_LAUNCH_DAEMON_PLIST" \
    "backend"

  remove_user_launchd_service \
    "$DAEMON_LAUNCH_AGENT_LABEL" \
    "$DAEMON_LAUNCH_AGENT_PLIST" \
    "backend launch agent"
}

remove_runner_service() {
  remove_system_launchd_service \
    "$RUNNER_LAUNCH_AGENT_LABEL" \
    "$RUNNER_LAUNCH_DAEMON_PLIST" \
    "runner"

  remove_user_launchd_service \
    "$RUNNER_LAUNCH_AGENT_LABEL" \
    "$RUNNER_LAUNCH_AGENT_PLIST" \
    "legacy runner launch agent"
}

remove_updater_service() {
  remove_system_launchd_service \
    "$UPDATER_LAUNCH_DAEMON_LABEL" \
    "$UPDATER_LAUNCH_DAEMON_PLIST" \
    "backend updater"
}

stop_local_web() {
  if [[ -f "$WEB_PID_FILE" ]]; then
    local pid=""
    pid="$(cat "$WEB_PID_FILE" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      log "Stopping local web UI (PID $pid)..."
      kill "$pid" >/dev/null 2>&1 || true
      sleep 1
    fi
    rm -f "$WEB_PID_FILE"
  fi

  if command -v pgrep >/dev/null 2>&1; then
    local pids=""
    pids="$(pgrep -f "$BIN_DIR/oore-web" 2>/dev/null || true)"
    if [[ -n "$pids" ]]; then
      log "Stopping local web UI processes..."
      echo "$pids" | xargs kill 2>/dev/null || true
      sleep 1
    fi
  fi
}

remove_local_web_launch_agent() {
  remove_user_launchd_service \
    "$WEB_LAUNCH_AGENT_LABEL" \
    "$WEB_LAUNCH_AGENT_PLIST" \
    "local web launch agent"
}

remove_local_web_systemd_user_service() {
  [[ "$(uname -s)" == "Linux" ]] || return 0

  if command -v systemctl >/dev/null 2>&1; then
    systemctl --user disable --now "$WEB_SYSTEMD_SERVICE_NAME" >/dev/null 2>&1 || true
  fi

  if [[ -f "$WEB_SYSTEMD_SERVICE_FILE" ]]; then
    log "Removing systemd user service: $WEB_SYSTEMD_SERVICE_FILE"
    rm -f "$WEB_SYSTEMD_SERVICE_FILE"
  fi

  if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload >/dev/null 2>&1 || true
    systemctl --user reset-failed "$WEB_SYSTEMD_SERVICE_NAME" >/dev/null 2>&1 || true
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
      # Remove install marker comment line(s) and the export PATH line
      local tmp="$rc.oore-uninstall.tmp"
      grep -v -e "# oore.build" -e "# Oore CI" "$rc" | grep -vF "$BIN_DIR" > "$tmp" || true
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
  local data_choice=""

  if [[ ! -d "$DATA_DIR" ]]; then
    return 0
  fi

  if is_noninteractive; then
    log "Removing data directory: $DATA_DIR"
    rm -rf "$DATA_DIR"
  else
    data_choice="$(
      prompt_select \
        "Application data found at $DATA_DIR. What do you want to do?" \
        "keep" \
        "keep:Keep data (recommended if you may reinstall)" \
        "remove:Remove all app data (includes database)"
    )"

    if [[ "$data_choice" == "remove" ]]; then
      log "Removing data directory: $DATA_DIR"
      rm -rf "$DATA_DIR"
    else
      log "Keeping data directory: $DATA_DIR"
    fi
  fi
}

main() {
  init_ui_theme

  if normalize_bool "$OORE_NONINTERACTIVE"; then
    :
  else
    if [[ "$?" -eq 2 ]]; then
      die 'OORE_NONINTERACTIVE must be one of: 1,0,true,false,yes,no,on,off.'
    fi
  fi

  print_uninstall_intro

  if [[ ! -d "$OORE_INSTALL_ROOT" ]] \
    && [[ ! -f "$DAEMON_LAUNCH_DAEMON_PLIST" ]] \
    && [[ ! -f "$UPDATER_LAUNCH_DAEMON_PLIST" ]] \
    && [[ ! -f "$RUNNER_LAUNCH_DAEMON_PLIST" ]] \
    && [[ ! -f "$DAEMON_LAUNCH_AGENT_PLIST" ]] \
    && [[ ! -f "$RUNNER_LAUNCH_AGENT_PLIST" ]] \
    && [[ ! -f "$WEB_LAUNCH_AGENT_PLIST" ]] \
    && [[ ! -f "$WEB_SYSTEMD_SERVICE_FILE" ]] \
    && ! system_launchd_job_is_loaded "$DAEMON_LAUNCH_AGENT_LABEL" \
    && ! system_launchd_job_is_loaded "$UPDATER_LAUNCH_DAEMON_LABEL" \
    && ! system_launchd_job_is_loaded "$RUNNER_LAUNCH_AGENT_LABEL" \
    && ! user_launchd_job_is_loaded "$DAEMON_LAUNCH_AGENT_LABEL" \
    && ! user_launchd_job_is_loaded "$RUNNER_LAUNCH_AGENT_LABEL" \
    && ! user_launchd_job_is_loaded "$WEB_LAUNCH_AGENT_LABEL" \
    && ! grep -rqF "$BIN_DIR" "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.profile" 2>/dev/null; then
    log "Oore CI does not appear to be installed."
    exit 0
  fi

  if ! is_noninteractive; then
    if ! prompt_yes_no "Continue with uninstall?" 'n'; then
      log "Uninstall cancelled."
      exit 0
    fi
  fi

  remove_runner_service
  remove_updater_service
  remove_daemon_launch_agent
  stop_daemon
  stop_local_web
  remove_local_web_launch_agent
  remove_local_web_systemd_user_service
  remove_from_path
  remove_install_dir
  remove_data_dir

  printf '\n%bOore CI has been uninstalled.%b\n' "$UI_BOLD$UI_SUCCESS" "$UI_RESET"
  log "Open a new terminal for PATH changes to take effect."
}

main "$@"
