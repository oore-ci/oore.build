#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WOODPECKER_CLI_VERSION="${WOODPECKER_CLI_VERSION:-3.13.0}"
CACHE_DIR="${HOME}/.cache/woodpecker-cli/${WOODPECKER_CLI_VERSION}"
CLI_BIN="${CACHE_DIR}/woodpecker-cli"

resolve_os() {
  local os
  os="$(uname -s)"
  case "$os" in
    Darwin) echo "darwin" ;;
    Linux) echo "linux" ;;
    *)
      echo "Unsupported OS for woodpecker-cli download: $os" >&2
      return 1
      ;;
  esac
}

resolve_arch() {
  local arch
  arch="$(uname -m)"
  case "$arch" in
    x86_64|amd64) echo "amd64" ;;
    arm64|aarch64) echo "arm64" ;;
    armv7l|armv7) echo "arm" ;;
    *)
      echo "Unsupported arch for woodpecker-cli download: $arch" >&2
      return 1
      ;;
  esac
}

ensure_cli() {
  if [[ -x "$CLI_BIN" ]]; then
    return 0
  fi

  local os arch asset url tmp archive found
  os="$(resolve_os)"
  arch="$(resolve_arch)"
  asset="woodpecker-cli_${os}_${arch}.tar.gz"
  url="https://github.com/woodpecker-ci/woodpecker/releases/download/v${WOODPECKER_CLI_VERSION}/${asset}"

  mkdir -p "$CACHE_DIR"
  tmp="$(mktemp -d)"
  archive="${tmp}/woodpecker-cli.tar.gz"

  echo "[lint-woodpecker] downloading woodpecker-cli v${WOODPECKER_CLI_VERSION} (${os}/${arch})"
  curl -fsSL -o "$archive" "$url"
  tar -xzf "$archive" -C "$tmp"

  found="$(find "$tmp" -type f -name woodpecker-cli | head -n1 || true)"
  if [[ -z "$found" ]]; then
    echo "Failed to locate woodpecker-cli binary in downloaded archive" >&2
    rm -rf "$tmp"
    return 1
  fi

  install -m 0755 "$found" "$CLI_BIN"
  rm -rf "$tmp"
}

ensure_cli

if [[ "$#" -eq 0 ]]; then
  set -- .woodpecker.yml
fi

echo "[lint-woodpecker] linting: $*"
"$CLI_BIN" lint "$@"
