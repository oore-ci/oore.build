#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEST_DIR="$(mktemp -d)"
trap 'rm -rf "$TEST_DIR"' EXIT

cmp -s "$ROOT_DIR/scripts/uninstall.sh" "$ROOT_DIR/apps/site/public/uninstall"

export HOME="$TEST_DIR/home"
export XDG_CONFIG_HOME="$TEST_DIR/xdg"
mkdir -p "$HOME"

# Load uninstaller functions without running main.
UNINSTALLER_LIB="$TEST_DIR/uninstall-lib.sh"
sed '$d' "$ROOT_DIR/scripts/uninstall.sh" > "$UNINSTALLER_LIB"
# shellcheck disable=SC1090
source "$UNINSTALLER_LIB"

mkdir -p "$WEB_SYSTEMD_USER_DIR"
touch "$WEB_SYSTEMD_SERVICE_FILE"
SYSTEMCTL_LOG="$TEST_DIR/systemctl.log"
systemctl() { printf '%s\n' "$*" >> "$SYSTEMCTL_LOG"; }
uname() { printf 'Linux\n'; }

remove_local_web_systemd_user_service
remove_local_web_systemd_user_service

[[ ! -e "$WEB_SYSTEMD_SERVICE_FILE" ]]
grep -q -- '--user disable --now oore-web.service' "$SYSTEMCTL_LOG"
grep -q -- '--user daemon-reload' "$SYSTEMCTL_LOG"
grep -q -- '--user reset-failed oore-web.service' "$SYSTEMCTL_LOG"

assert_system_uninstall_uses_root_tools() (
  local source="$1"
  local name="$2"
  local case_dir="$TEST_DIR/$name"
  local uninstaller_lib="$case_dir/uninstall-lib.sh"
  local sudo_log="$case_dir/sudo.log"
  local daemon_loaded=1
  local runner_loaded=1

  export HOME="$case_dir/home"
  export OORE_INSTALL_ROOT="$case_dir/install"
  mkdir -p "$HOME" "$OORE_INSTALL_ROOT/bin"
  sed '$d' "$source" > "$uninstaller_lib"
  # shellcheck disable=SC1090
  source "$uninstaller_lib"

  DAEMON_LAUNCH_DAEMON_PLIST="$case_dir/build.oore.oored.plist"
  RUNNER_LAUNCH_DAEMON_PLIST="$case_dir/build.oore.oore-runner.plist"
  touch "$DAEMON_LAUNCH_DAEMON_PLIST" "$RUNNER_LAUNCH_DAEMON_PLIST"
  sudo() {
    printf '%s\n' "$*" >> "$sudo_log"

    if [[ "${1:-}" == "-v" ]]; then
      return 0
    fi
    if [[ "${1:-}" == "-n" ]]; then
      shift
    fi

    if [[ "${1:-}" == "/bin/launchctl" && "${2:-}" == "bootout" ]]; then
      case "${3:-}" in
        system/build.oore.oored) daemon_loaded=0 ;;
        system/build.oore.oore-runner) runner_loaded=0 ;;
      esac
    elif [[ "${1:-}" == "/bin/launchctl" && "${2:-}" == "print" ]]; then
      case "${3:-}" in
        system/build.oore.oored) [[ "$daemon_loaded" -eq 1 ]] ;;
        system/build.oore.oore-runner) [[ "$runner_loaded" -eq 1 ]] ;;
        *) return 1 ;;
      esac
      return
    elif [[ "${1:-}" == "/bin/rm" ]]; then
      command /bin/rm "${@:2}"
    fi

    return 0
  }
  launchctl() {
    if [[ "${1:-}" == "print" ]]; then
      case "${2:-}" in
        system/build.oore.oored) [[ "$daemon_loaded" -eq 1 ]] ;;
        system/build.oore.oore-runner) [[ "$runner_loaded" -eq 1 ]] ;;
        *) return 1 ;;
      esac
      return
    fi
    return 0
  }

  remove_runner_service
  remove_daemon_launch_agent

  grep -q -- '^-v$' "$sudo_log"
  grep -q -- '^-n /bin/launchctl bootout system/build.oore.oore-runner$' "$sudo_log"
  grep -q -- '^-n /bin/launchctl remove build.oore.oore-runner$' "$sudo_log"
  grep -q -- '^-n /bin/launchctl print system/build.oore.oore-runner$' "$sudo_log"
  grep -q -- "^-n /bin/rm -f $RUNNER_LAUNCH_DAEMON_PLIST$" "$sudo_log"
  grep -q -- '^-n /bin/launchctl bootout system/build.oore.oored$' "$sudo_log"
  grep -q -- '^-n /bin/launchctl remove build.oore.oored$' "$sudo_log"
  grep -q -- '^-n /bin/launchctl print system/build.oore.oored$' "$sudo_log"
  grep -q -- "^-n /bin/rm -f $DAEMON_LAUNCH_DAEMON_PLIST$" "$sudo_log"
  [[ ! -e "$RUNNER_LAUNCH_DAEMON_PLIST" ]]
  [[ ! -e "$DAEMON_LAUNCH_DAEMON_PLIST" ]]
)

assert_legacy_mode_without_system_jobs_skips_sudo() (
  local source="$1"
  local name="$2"
  local case_dir="$TEST_DIR/no-system-job-$name"
  local uninstaller_lib="$case_dir/uninstall-lib.sh"
  local sudo_log="$case_dir/sudo.log"

  export HOME="$case_dir/home"
  export OORE_INSTALL_ROOT="$case_dir/install"
  mkdir -p "$HOME" "$OORE_INSTALL_ROOT/bin"
  sed '$d' "$source" > "$uninstaller_lib"
  # shellcheck disable=SC1090
  source "$uninstaller_lib"

  printf 'all\n' > "$OORE_INSTALL_ROOT/INSTALL_MODE"
  DAEMON_LAUNCH_DAEMON_PLIST="$case_dir/missing-oored.plist"
  RUNNER_LAUNCH_DAEMON_PLIST="$case_dir/missing-runner.plist"
  sudo() {
    printf '%s\n' "$*" >> "$sudo_log"
    return 1
  }
  launchctl() {
    [[ "${1:-}" != "print" ]]
  }

  remove_runner_service
  remove_daemon_launch_agent

  [[ ! -e "$sudo_log" ]]
)

assert_loaded_runner_is_verified_before_plist_removal() (
  local source="$1"
  local name="$2"
  local case_dir="$TEST_DIR/failure-$name"
  local uninstaller_lib="$case_dir/uninstall-lib.sh"
  local sudo_log="$case_dir/sudo.log"

  export HOME="$case_dir/home"
  export OORE_INSTALL_ROOT="$case_dir/install"
  mkdir -p "$HOME" "$OORE_INSTALL_ROOT/bin"
  sed '$d' "$source" > "$uninstaller_lib"
  # shellcheck disable=SC1090
  source "$uninstaller_lib"

  printf 'all\n' > "$OORE_INSTALL_ROOT/INSTALL_MODE"
  RUNNER_LAUNCH_DAEMON_PLIST="$case_dir/build.oore.oore-runner.plist"
  touch "$RUNNER_LAUNCH_DAEMON_PLIST"
  sudo() {
    printf '%s\n' "$*" >> "$sudo_log"
    if [[ "${1:-}" == "-v" ]]; then
      return 0
    fi
    if [[ "${1:-}" == "-n" && "${2:-}" == "/bin/launchctl" && "${3:-}" == "print" ]]; then
      return 0
    fi
    return 0
  }
  launchctl() {
    [[ "${1:-}" == "print" && "${2:-}" == "system/$RUNNER_LAUNCH_AGENT_LABEL" ]]
  }

  if (remove_runner_service); then
    echo "[uninstall-acceptance] $name removed a plist while the runner job stayed loaded" >&2
    exit 1
  fi
  [[ -d "$OORE_INSTALL_ROOT" ]]
  [[ -f "$RUNNER_LAUNCH_DAEMON_PLIST" ]]
  if grep -q -- '/bin/rm ' "$sudo_log"; then
    echo "[uninstall-acceptance] $name attempted plist deletion before verifying unload" >&2
    exit 1
  fi
  grep -q -- "^-n /bin/launchctl print system/$RUNNER_LAUNCH_AGENT_LABEL$" "$sudo_log"
)

assert_loaded_user_job_is_verified_before_plist_removal() (
  local source="$1"
  local name="$2"
  local component="$3"
  local case_dir="$TEST_DIR/user-failure-$name-$component"
  local uninstaller_lib="$case_dir/uninstall-lib.sh"
  local sudo_log="$case_dir/sudo.log"
  local plist="$case_dir/$component.plist"
  local stubborn_label=""
  local remover=""

  export HOME="$case_dir/home"
  export OORE_INSTALL_ROOT="$case_dir/install"
  mkdir -p "$HOME" "$OORE_INSTALL_ROOT/bin"
  sed '$d' "$source" > "$uninstaller_lib"
  # shellcheck disable=SC1090
  source "$uninstaller_lib"

  DAEMON_LAUNCH_DAEMON_PLIST="$case_dir/missing-system-oored.plist"
  RUNNER_LAUNCH_DAEMON_PLIST="$case_dir/missing-system-runner.plist"
  DAEMON_LAUNCH_AGENT_PLIST="$case_dir/missing-user-oored.plist"
  RUNNER_LAUNCH_AGENT_PLIST="$case_dir/missing-user-runner.plist"
  WEB_LAUNCH_AGENT_PLIST="$case_dir/missing-user-web.plist"
  case "$component" in
    daemon)
      stubborn_label="$DAEMON_LAUNCH_AGENT_LABEL"
      DAEMON_LAUNCH_AGENT_PLIST="$plist"
      remover=remove_daemon_launch_agent
      ;;
    runner)
      stubborn_label="$RUNNER_LAUNCH_AGENT_LABEL"
      RUNNER_LAUNCH_AGENT_PLIST="$plist"
      remover=remove_runner_service
      ;;
    web)
      stubborn_label="$WEB_LAUNCH_AGENT_LABEL"
      WEB_LAUNCH_AGENT_PLIST="$plist"
      remover=remove_local_web_launch_agent
      ;;
  esac
  touch "$plist"
  sudo() {
    printf '%s\n' "$*" >> "$sudo_log"
    return 1
  }
  launchctl() {
    if [[ "${1:-}" == "print" ]]; then
      [[ "${2:-}" == "gui/$(id -u)/$stubborn_label" ]]
      return
    fi
    return 0
  }

  if ("$remover" >/dev/null 2>&1); then
    echo "[uninstall-acceptance] $name removed the $component plist while its user job stayed loaded" >&2
    exit 1
  fi
  [[ -d "$OORE_INSTALL_ROOT" ]]
  [[ -f "$plist" ]]
  [[ ! -e "$sudo_log" ]]
)

assert_system_uninstall_uses_root_tools "$ROOT_DIR/scripts/uninstall.sh" scripts
assert_system_uninstall_uses_root_tools "$ROOT_DIR/apps/site/public/uninstall" public
assert_legacy_mode_without_system_jobs_skips_sudo "$ROOT_DIR/scripts/uninstall.sh" scripts
assert_legacy_mode_without_system_jobs_skips_sudo "$ROOT_DIR/apps/site/public/uninstall" public
assert_loaded_runner_is_verified_before_plist_removal "$ROOT_DIR/scripts/uninstall.sh" scripts
assert_loaded_runner_is_verified_before_plist_removal "$ROOT_DIR/apps/site/public/uninstall" public
for component in daemon runner web; do
  assert_loaded_user_job_is_verified_before_plist_removal "$ROOT_DIR/scripts/uninstall.sh" scripts "$component"
  assert_loaded_user_job_is_verified_before_plist_removal "$ROOT_DIR/apps/site/public/uninstall" public "$component"
done

echo "[uninstall-acceptance] passed"
