#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${OORE_REPO_URL:-https://github.com/devaryakjha/oore.build.git}"
INSTALL_ROOT="${OORE_INSTALL_ROOT:-$HOME/.oore}"
SOURCE_DIR="${OORE_SOURCE_DIR:-$INSTALL_ROOT/src/oore.build}"
START_DAEMON="${OORE_START_DAEMON:-true}"
AUTO_INSTALL_DEPS="${OORE_AUTO_INSTALL_DEPS:-true}"

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

install_rust() {
  log 'Installing Rust toolchain (rustup)...'
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
}

install_bun() {
  log 'Installing Bun...'
  curl -fsSL https://bun.sh/install | bash
  export BUN_INSTALL="${BUN_INSTALL:-$HOME/.bun}"
  export PATH="$BUN_INSTALL/bin:$PATH"
}

install_fvm() {
  if ! have_cmd brew; then
    die 'fvm is required. Install Homebrew first, then run: brew tap leoafarias/fvm && brew install fvm'
  fi
  log 'Installing FVM via Homebrew...'
  brew tap leoafarias/fvm
  brew install fvm
}

ensure_dependency() {
  local cmd="$1"
  local installer="$2"

  if have_cmd "$cmd"; then
    return 0
  fi

  if [[ "$AUTO_INSTALL_DEPS" != "true" ]]; then
    die "$cmd is required. Install it and rerun."
  fi

  "$installer"

  if ! have_cmd "$cmd"; then
    die "Failed to install required dependency: $cmd"
  fi
}

if [[ "$(uname -s)" != "Darwin" ]]; then
  die 'oore.build V1 backend installer currently supports macOS only.'
fi

ensure_dependency git true
ensure_dependency curl true
ensure_dependency rustc install_rust
ensure_dependency cargo install_rust
ensure_dependency bun install_bun
ensure_dependency fvm install_fvm

if ! xcode-select -p >/dev/null 2>&1; then
  log 'Xcode Command Line Tools are missing. Triggering installer prompt...'
  xcode-select --install || true
  die 'Install Xcode Command Line Tools, then rerun this installer.'
fi

mkdir -p "$(dirname "$SOURCE_DIR")"

if [[ -d "$SOURCE_DIR/.git" ]]; then
  log "Updating existing checkout at $SOURCE_DIR"
  git -C "$SOURCE_DIR" fetch --depth=1 origin
  git -C "$SOURCE_DIR" reset --hard origin/main
else
  log "Cloning repository into $SOURCE_DIR"
  git clone --depth=1 "$REPO_URL" "$SOURCE_DIR"
fi

cd "$SOURCE_DIR"

log 'Installing frontend dependencies (bun install)...'
bun install

log 'Building Rust binaries (oored, oore)...'
cargo build -p oored -p oore

if [[ "$START_DAEMON" == "true" ]]; then
  mkdir -p "$INSTALL_ROOT/logs"
  DAEMON_LOG="$INSTALL_ROOT/logs/oored.log"

  log 'Starting oored in background on 127.0.0.1:8787...'
  nohup cargo run -p oored -- run --listen 127.0.0.1:8787 >"$DAEMON_LOG" 2>&1 &
  sleep 1

  if curl -fsS http://127.0.0.1:8787/healthz >/dev/null 2>&1; then
    log 'Daemon is healthy.'
  else
    log "Daemon startup in progress. Check logs: $DAEMON_LOG"
  fi
fi

cat <<'DONE'

Installation complete.

Next steps:
  1) Generate setup token:
     cargo run -p oore -- setup open --ttl 15m

  2) Open setup UI and complete OIDC setup:
     http://localhost:3000/setup

  3) Trigger a build:
     In default mode, oored runs an embedded local runner automatically.

Optional environment variables:
  OORE_START_DAEMON=false        # Skip daemon startup
  OORE_AUTO_INSTALL_DEPS=false   # Do not auto-install missing tools
  OORE_SOURCE_DIR=/custom/path   # Override checkout location
  OORE_REPO_URL=https://...      # Override git source

DONE
