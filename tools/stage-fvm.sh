#!/usr/bin/env bash
set -euo pipefail

FVM_VERSION="4.1.2"
FVM_REPOSITORY="conceptadev/fvm"
FVM_ARM64_SHA256="0b2a146986c51f06331f135f0bdf2a202eb57f55d7edd420c9078e8520e4c033"
FVM_X64_SHA256="7bbfcb6883ea67ce532163704f5625eba7ecf340084be707cde71a28fefff1d8"

usage() {
  echo "usage: $0 <arm64|x86_64> <release-stage>" >&2
  exit 2
}

[[ $# -eq 2 ]] || usage

release_arch="$1"
stage="$2"
case "$release_arch" in
  arm64)
    fvm_arch="arm64"
    expected_sha="$FVM_ARM64_SHA256"
    ;;
  x86_64)
    fvm_arch="x64"
    expected_sha="$FVM_X64_SHA256"
    ;;
  *)
    usage
    ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

archive="fvm-${FVM_VERSION}-macos-${fvm_arch}.tar.gz"
url="https://github.com/${FVM_REPOSITORY}/releases/download/${FVM_VERSION}/${archive}"
curl -fsSL --retry 3 --connect-timeout 10 --max-time 300 "$url" -o "$tmp/$archive"
printf '%s  %s\n' "$expected_sha" "$tmp/$archive" | shasum -a 256 -c -

if tar -tzf "$tmp/$archive" | grep -qE '^/|^\.\.$|^\.\./|/\.\.$|/\.\./'; then
  echo "FVM archive contains an unsafe path" >&2
  exit 1
fi

mkdir -p "$tmp/extract" "$stage/bin" "$stage/libexec"
tar -xzf "$tmp/$archive" -C "$tmp/extract"
[[ -f "$tmp/extract/fvm/fvm" ]] || {
  echo "FVM archive is missing fvm/fvm" >&2
  exit 1
}

rm -rf "$stage/libexec/fvm"
cp -R "$tmp/extract/fvm" "$stage/libexec/fvm"
chmod +x "$stage/libexec/fvm/fvm"

cat > "$stage/bin/fvm" <<'EOF'
#!/bin/sh
set -eu
root="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
exec "$root/libexec/fvm/fvm" "$@"
EOF
chmod +x "$stage/bin/fvm"
