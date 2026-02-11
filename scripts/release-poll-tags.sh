#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GIT_REMOTE="${OORE_GIT_REMOTE:-origin}"
TAG_PATTERN="${OORE_TAG_PATTERN:-v*.*.*}"
STATE_DIR="${OORE_RELEASE_STATE_DIR:-$HOME/.oore/release-runner}"
STATE_FILE="$STATE_DIR/published-tags.txt"

log() {
  printf '[release-poll] %s\n' "$*"
}

die() {
  printf '[release-poll] ERROR: %s\n' "$*" >&2
  exit 1
}

mkdir -p "$STATE_DIR"
touch "$STATE_FILE"

git -C "$ROOT_DIR" fetch "$GIT_REMOTE" --tags

tags="$(git -C "$ROOT_DIR" tag -l "$TAG_PATTERN" --sort=v:refname)"
if [[ -z "$tags" ]]; then
  log "No tags found matching pattern: $TAG_PATTERN"
  exit 0
fi

while IFS= read -r tag; do
  [[ -n "$tag" ]] || continue
  if grep -Fxq "$tag" "$STATE_FILE"; then
    continue
  fi

  log "Publishing new tag: $tag"
  "$ROOT_DIR/scripts/release-local.sh" "$tag"
  printf '%s\n' "$tag" >> "$STATE_FILE"
done <<< "$tags"

log 'Tag polling completed.'
