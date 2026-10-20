#!/usr/bin/env bash
set -euo pipefail

# Set the workspace package version in Cargo.toml and the matching
# workspace entries in Cargo.lock. Registry and git packages keep their
# versions because only path packages (no `source`) are rewritten.

if [[ "$#" -ne 1 ]]; then
  echo "Usage: $0 <X.Y.Z>" >&2
  exit 2
fi

next="$1"
[[ "$next" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "The next version must use X.Y.Z: $next" >&2
  exit 1
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

current="$(awk '
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
[[ "$current" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "Cargo.toml has no valid workspace package version." >&2
  exit 1
}
if [[ "$current" == "$next" ]]; then
  echo "The workspace is already at $next."
  exit 0
fi

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT

awk -v current="$current" -v next_version="$next" '
  $0 == "[workspace.package]" { inside = 1; print; next }
  inside && /^\[/ { inside = 0 }
  inside && !done && $0 == "version = \"" current "\"" {
    print "version = \"" next_version "\""
    done = 1
    next
  }
  { print }
  END { if (!done) exit 1 }
' Cargo.toml > "$temporary/Cargo.toml" || {
  echo "Failed to update the workspace version in Cargo.toml." >&2
  exit 1
}

awk -v current="$current" -v next_version="$next" '
  function flush(   i) {
    for (i = 1; i <= count; i++) {
      if (!has_source && i == version_line) {
        print "version = \"" next_version "\""
        changed += 1
      } else {
        print block[i]
      }
    }
    count = 0
    has_source = 0
    version_line = 0
  }
  $0 == "[[package]]" { flush() }
  {
    block[++count] = $0
    if ($0 ~ /^source = /) has_source = 1
    if ($0 == "version = \"" current "\"") version_line = count
  }
  END {
    flush()
    if (!changed) exit 1
  }
' Cargo.lock > "$temporary/Cargo.lock" || {
  echo "Cargo.lock has no workspace packages at $current." >&2
  exit 1
}

cat "$temporary/Cargo.toml" > Cargo.toml
cat "$temporary/Cargo.lock" > Cargo.lock
echo "Bumped the workspace from $current to $next."
