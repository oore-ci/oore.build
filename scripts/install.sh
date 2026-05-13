#!/usr/bin/env bash
set -euo pipefail

OORE_VERSION="${OORE_VERSION:-latest}"
OORE_CHANNEL="${OORE_CHANNEL:-stable}"
OORE_INSTALL_MODE="${OORE_INSTALL_MODE:-full}"
OORE_INSTALL_ROOT="${OORE_INSTALL_ROOT:-$HOME/.oore}"
OORE_GITHUB_REPO="${OORE_GITHUB_REPO:-devaryakjha/oore.build}"
OORE_RELEASE_BASE_URL="${OORE_RELEASE_BASE_URL:-https://github.com/$OORE_GITHUB_REPO/releases/download}"
OORE_RELEASE_MANIFEST_URL="${OORE_RELEASE_MANIFEST_URL:-https://api.github.com/repos/$OORE_GITHUB_REPO/releases/latest}"
OORE_RELEASES_LIST_URL="${OORE_RELEASES_LIST_URL:-https://api.github.com/repos/$OORE_GITHUB_REPO/releases?per_page=100}"
OORE_NONINTERACTIVE="${OORE_NONINTERACTIVE:-0}"
OORE_START_DAEMON="${OORE_START_DAEMON:-}"
OORE_HOSTED_UI="${OORE_HOSTED_UI:-https://ci.oore.build}"
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
UI_RESET=""
UI_BOLD=""
UI_DIM=""
UI_ACCENT=""
UI_SUCCESS=""
UI_WARNING=""
UI_ERROR=""

print_help() {
  cat <<'EOF'
Oore CI installer

Usage:
  ./scripts/install.sh
  ./scripts/install.sh --help

Environment overrides:
  OORE_VERSION               Release tag or "latest" (default: latest)
  OORE_CHANNEL               Release channel for latest resolution: stable|beta|alpha (default: stable)
  OORE_INSTALL_MODE          Install mode: full|frontend (default: full)
  OORE_INSTALL_ROOT          Install root (default: ~/.oore)
  OORE_NONINTERACTIVE        Non-interactive mode (true/false)
  OORE_START_DAEMON          Start daemon in non-interactive mode (true/false)
  OORE_DAEMON_URL            Daemon URL used by full-mode setup helpers (default: http://127.0.0.1:8787)
  OORE_WEB_BACKEND_URL       Backend URL proxied by oore-web (default: OORE_DAEMON_URL)
  OORE_LOCAL_WEB_MODE        Local web behavior in non-interactive mode: off|run|login
  OORE_LOCAL_WEB_LISTEN      Local web listen address (default: 127.0.0.1:4173)
  OORE_HOSTED_UI             Hosted UI URL (default: https://ci.oore.build)
  OORE_GITHUB_REPO           GitHub repo (default: devaryakjha/oore.build)
  OORE_RELEASE_BASE_URL      Release asset base URL (default: GitHub Releases download base)
  OORE_RELEASE_MANIFEST_URL  Release metadata URL for latest tag resolution (default: GitHub Releases API)
  OORE_RELEASES_LIST_URL     Release list URL for prerelease channel resolution (default: GitHub Releases API list)
EOF
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

print_install_intro() {
  printf '\n'
  print_ascii_banner
  printf '%bOore CI Installer%b\n' "$UI_BOLD$UI_ACCENT" "$UI_RESET"
  printf '%b----------------------------------------%b\n' "$UI_DIM" "$UI_RESET"
  printf '  Mode:          %s\n' "$OORE_INSTALL_MODE"
  printf '  Install root:  %s\n' "$OORE_INSTALL_ROOT"
  if [[ "$OORE_VERSION" == "latest" ]]; then
    printf '  Release:       latest (%s channel)\n' "$OORE_CHANNEL"
  else
    printf '  Release:       %s\n' "$OORE_VERSION"
  fi
  if [[ "$OORE_INSTALL_MODE" == "frontend" ]]; then
    printf '  Backend URL:   %s\n' "$WEB_BACKEND_URL"
    printf '  Web listen:    %s\n' "$OORE_LOCAL_WEB_LISTEN"
  fi
  printf '  Hosted setup:  %s\n' "$OORE_HOSTED_UI"
  printf '%b----------------------------------------%b\n' "$UI_DIM" "$UI_RESET"
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
      die "Unsupported operating system: $(uname -s). Full install supports macOS; frontend install supports macOS and Linux."
      ;;
  esac
}

validate_install_mode() {
  case "${OORE_INSTALL_MODE:-}" in
    full|frontend)
      return 0
      ;;
    *)
      die 'OORE_INSTALL_MODE must be one of: full,frontend.'
      ;;
  esac
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

resolve_latest_channel_tag_from_list() {
  local json_file="$1"
  local channel="$2"
  local want_re=""

  case "$channel" in
    alpha) want_re='-alpha\.' ;;
    beta) want_re='-beta\.' ;;
    *) die "resolve_latest_channel_tag_from_list: unsupported channel: $channel" ;;
  esac

  # The GitHub API returns releases ordered newest-first.
  # We'll pick the first matching prerelease tag for the requested channel.
  local tag=""
  local draft=""
  local prerelease=""
  local line=""
  while IFS= read -r line; do
    if [[ -z "$tag" ]]; then
      tag="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' <<<"$line" | head -n1)"
    fi
    if [[ -z "$draft" ]]; then
      draft="$(sed -n 's/.*"draft"[[:space:]]*:[[:space:]]*\([^,}]*\).*/\1/p' <<<"$line" | head -n1)"
    fi
    if [[ -z "$prerelease" ]]; then
      prerelease="$(sed -n 's/.*"prerelease"[[:space:]]*:[[:space:]]*\([^,}]*\).*/\1/p' <<<"$line" | head -n1)"
    fi

    if [[ -n "$tag" && -n "$draft" && -n "$prerelease" ]]; then
      if [[ "$draft" == "false" && "$prerelease" == "true" ]] && echo "$tag" | grep -qE -- "$want_re"; then
        printf '%s' "$tag"
        return 0
      fi
      tag=""
      draft=""
      prerelease=""
    fi
  done < "$json_file"

  return 1
}

resolve_latest_stable_tag_from_list() {
  local json_file="$1"

  # The GitHub API returns releases ordered newest-first.
  # We'll pick the first non-draft, non-prerelease tag.
  local tag=""
  local draft=""
  local prerelease=""
  local line=""
  while IFS= read -r line; do
    if [[ -z "$tag" ]]; then
      tag="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' <<<"$line" | head -n1)"
    fi
    if [[ -z "$draft" ]]; then
      draft="$(sed -n 's/.*"draft"[[:space:]]*:[[:space:]]*\([^,}]*\).*/\1/p' <<<"$line" | head -n1)"
    fi
    if [[ -z "$prerelease" ]]; then
      prerelease="$(sed -n 's/.*"prerelease"[[:space:]]*:[[:space:]]*\([^,}]*\).*/\1/p' <<<"$line" | head -n1)"
    fi

    if [[ -n "$tag" && -n "$draft" && -n "$prerelease" ]]; then
      if [[ "$draft" == "false" && "$prerelease" == "false" ]]; then
        printf '%s' "$tag"
        return 0
      fi
      tag=""
      draft=""
      prerelease=""
    fi
  done < "$json_file"

  return 1
}

resolve_release_tag() {
  local tag=""
  if [[ "$OORE_VERSION" == "latest" ]]; then
    if [[ "$OORE_CHANNEL" == "stable" ]]; then
      local manifest_file="$TMP_DIR/latest.json"
      if curl -fsSL --retry 3 --output "$manifest_file" "$OORE_RELEASE_MANIFEST_URL"; then
        # GitHub API returns "tag_name": "vX.Y.Z"
        tag="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest_file" | head -n1)"
        [[ -n "$tag" ]] || die "Unable to parse tag from release manifest: $OORE_RELEASE_MANIFEST_URL"
      else
        # GitHub returns 404 when there are no releases yet.
        local list_file="$TMP_DIR/releases.json"
        log "No stable release manifest found. Falling back to release list."
        curl -fsSL --retry 3 --output "$list_file" "$OORE_RELEASES_LIST_URL" \
          || die "Unable to fetch release list: $OORE_RELEASES_LIST_URL"

        tag="$(resolve_latest_stable_tag_from_list "$list_file" || true)"
        [[ -n "$tag" ]] || die "No stable releases found. Try a prerelease channel: OORE_CHANNEL=beta (or alpha)."
      fi
    else
      local list_file="$TMP_DIR/releases.json"
      curl -fsSL --retry 3 --output "$list_file" "$OORE_RELEASES_LIST_URL" \
        || die "Unable to fetch release list: $OORE_RELEASES_LIST_URL"

      tag="$(resolve_latest_channel_tag_from_list "$list_file" "$OORE_CHANNEL" || true)"
      [[ -n "$tag" ]] || die "Unable to resolve latest $OORE_CHANNEL release from: $OORE_RELEASES_LIST_URL"
    fi
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
    full)
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
  curl -fsSL --retry 3 --output "$TMP_DIR/$archive_name" "$archive_url" \
    || die "Failed to download release archive: $archive_url"
  curl -fsSL --retry 3 --output "$TMP_DIR/$checksum_name" "$checksum_url" \
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

install_binaries() {
  local archive_name
  local extract_dir="$TMP_DIR/extract"
  archive_name="$(release_archive_name)"

  mkdir -p "$extract_dir"
  tar -xzf "$TMP_DIR/$archive_name" -C "$extract_dir"

  if [[ "$OORE_INSTALL_MODE" == "full" ]]; then
    [[ -f "$extract_dir/bin/oored" ]] || die "Release archive is missing bin/oored."
    [[ -f "$extract_dir/bin/oore" ]] || die "Release archive is missing bin/oore."
  fi
  [[ -f "$extract_dir/bin/oore-web" ]] || die "Release archive is missing bin/oore-web."
  [[ -d "$extract_dir/web-dist" ]] || die "Release archive is missing web-dist."
  [[ -f "$extract_dir/VERSION" ]] || die "Release archive is missing VERSION."

  mkdir -p "$BIN_DIR" "$LOG_DIR"
  if [[ "$OORE_INSTALL_MODE" == "full" ]]; then
    cp "$extract_dir/bin/oored" "$BIN_DIR/oored"
    cp "$extract_dir/bin/oore" "$BIN_DIR/oore"
    chmod +x "$BIN_DIR/oored" "$BIN_DIR/oore"
  fi
  cp "$extract_dir/bin/oore-web" "$WEB_BINARY"
  chmod +x "$WEB_BINARY"
  rm -rf "$WEB_DIST_DIR"
  cp -R "$extract_dir/web-dist" "$WEB_DIST_DIR"

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
    --backend-url "$WEB_BACKEND_URL" \
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
      <string>$WEB_BACKEND_URL</string>
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

install_local_web_systemd_user_service() {
  if ! has_local_web_bundle; then
    log "Cannot install systemd user service: bundled web UI is unavailable."
    return 1
  fi
  if ! have_cmd systemctl; then
    log "Cannot install systemd user service: systemctl is unavailable."
    return 1
  fi

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

[Install]
WantedBy=default.target
EOF

  systemctl --user daemon-reload
  systemctl --user enable --now "$WEB_SYSTEMD_SERVICE_NAME"
  log "Installed systemd user service: $WEB_SYSTEMD_SERVICE_NAME"
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
      start_local_web || true
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
          open "${LOCAL_WEB_URL}/setup" >/dev/null 2>&1 || true
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
      printf 'Your Oore instance is ready.\n\n'
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

  if [[ "$OORE_INSTALL_MODE" == "frontend" ]]; then
    printf 'Frontend is installed at %s\n' "$LOCAL_WEB_URL"
    printf 'Backend proxy target: %s\n\n' "$WEB_BACKEND_URL"
    if "$local_web_running"; then
      printf 'Frontend status: running\n'
    else
      printf 'Start the frontend:\n'
      printf '  oore-web --listen %s --backend-url %s\n' "$OORE_LOCAL_WEB_LISTEN" "$WEB_BACKEND_URL"
    fi
    printf '\nPut your HTTPS reverse proxy / Warpgate target in front of %s.\n' "$LOCAL_WEB_URL"
    printf 'In the UI, add an instance with Backend URL empty so browser API calls use this frontend proxy.\n'
    printf '\nDocs: https://docs.oore.build\n'
    return 0
  fi

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
  init_ui_theme

  validate_install_mode
  validate_local_web_mode
  validate_channel

  if normalize_bool "$OORE_NONINTERACTIVE"; then
    :
  else
    if [[ "$?" -eq 2 ]]; then
      die 'OORE_NONINTERACTIVE must be one of: 1,0,true,false,yes,no,on,off.'
    fi
  fi

  if ! is_noninteractive; then
    print_install_intro
  fi

  detect_os
  if [[ "$OORE_INSTALL_MODE" == "full" && "$RELEASE_OS" != "darwin" ]]; then
    die 'Oore CI V1 backend installer currently supports macOS only.'
  fi

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
  elif has_local_web_bundle; then
    step_done "$BIN_DIR/{oored,oore,oore-web}"
  else
    step_done "$BIN_DIR/{oored,oore}"
  fi

  ensure_on_path

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
