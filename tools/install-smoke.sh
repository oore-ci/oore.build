#!/usr/bin/env bash
# Installs the CLI built from this checkout with scripts/install.sh over the
# unsigned loopback acceptance path, reinstalls it as an update, and removes it
# with scripts/uninstall.sh. Everything stays inside a temporary HOME.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

log() {
  printf '[install-smoke] %s\n' "$*"
}

fail() {
  printf '[install-smoke] FAILED: %s\n' "$*" >&2
  exit 1
}

[[ "$(uname -s)" == Darwin ]] || fail 'the installer supports macOS only.'
case "$(uname -m)" in
  arm64 | aarch64) arch=arm64 ;;
  x86_64 | amd64) arch=x86_64 ;;
  *) fail "unsupported architecture $(uname -m)" ;;
esac

version="$(awk '
  $0 == "[workspace.package]" { inside = 1; next }
  inside && /^\[/ { exit }
  inside && /^[[:space:]]*version[[:space:]]*=/ {
    value = $0
    sub(/^[^=]*=[[:space:]]*"/, "", value)
    sub(/"[[:space:]]*$/, "", value)
    print value
    exit
  }
' Cargo.toml)"
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "unexpected workspace version: $version"

log "Building oore $version..."
cargo build -p oore --locked
cli="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/debug/oore"
[[ -x "$cli" ]] || fail "cargo did not produce $cli"

work="$(mktemp -d "${TMPDIR:-/tmp}/oore-install-smoke.XXXXXX")"
work="$(cd "$work" && pwd -P)"
server_pid=""
cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT

archive="oore-cli_${version}_darwin_${arch}.tar.gz"
release_dir="$work/releases/v$version"
mkdir -p "$work/stage/bin" "$release_dir"
cp "$cli" "$work/stage/bin/oore"
printf '%s\n' "$version" > "$work/stage/VERSION"
tar -czf "$release_dir/$archive" -C "$work/stage" bin/oore VERSION
(cd "$release_dir" && shasum -a 256 "$archive" > "oore_${version}_checksums.txt")

port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
python3 -m http.server "$port" --bind 127.0.0.1 --directory "$work/releases" \
  > "$work/server.log" 2>&1 &
server_pid=$!
for _ in $(seq 1 50); do
  curl --disable -fsS --noproxy '*' -o /dev/null \
    "http://127.0.0.1:$port/v$version/oore_${version}_checksums.txt" 2>/dev/null && break
  sleep 0.1
done

home="$work/home"
install_root="$home/.oore"
mkdir -m 0700 "$home"
run_installer() {
  /usr/bin/env -i \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
    HOME="$home" \
    TMPDIR="$work" \
    OORE_INSTALL_ROOT="$install_root" \
    OORE_VERSION="$version" \
    OORE_RELEASE_BASE_URL="http://127.0.0.1:$port" \
    OORE_ALLOW_UNSIGNED_LOCAL_RELEASE=true \
    OORE_MODIFY_PATH=false \
    /bin/bash scripts/install.sh < /dev/null
}

log 'Installing...'
run_installer
[[ -x "$install_root/bin/oore" ]] || fail 'the CLI was not installed'
cmp -s "$cli" "$install_root/bin/oore" || fail 'the installed CLI differs from the build'
[[ "$(< "$install_root/VERSION")" == "$version" ]] || fail 'VERSION metadata is wrong'
[[ "$(< "$install_root/CHANNEL")" == stable ]] || fail 'CHANNEL metadata is wrong'
[[ "$(< "$install_root/BOOTSTRAP_ARCHIVE")" == "$archive" ]] \
  || fail 'BOOTSTRAP_ARCHIVE metadata is wrong'
"$install_root/bin/oore" --help > /dev/null || fail 'the installed CLI does not start'

log 'Reinstalling over the existing installation...'
run_installer
cmp -s "$cli" "$install_root/bin/oore" || fail 'the reinstalled CLI differs from the build'

log 'Uninstalling...'
/usr/bin/env -i \
  PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
  HOME="$home" \
  OORE_INSTALL_ROOT="$install_root" \
  /bin/bash scripts/uninstall.sh --yes < /dev/null
[[ ! -e "$install_root/bin/oore" ]] || fail 'uninstall left the CLI behind'

log 'Installer, update and uninstaller passed.'
