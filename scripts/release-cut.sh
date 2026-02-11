#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION_INPUT="${1:-${VERSION:-}}"
GIT_REMOTE="${OORE_GIT_REMOTE:-origin}"
RELEASE_BRANCH="${OORE_RELEASE_BRANCH:-master}"

VERSION=""
TAG=""

log() {
  printf '[release-cut] %s\n' "$*"
}

die() {
  printf '[release-cut] ERROR: %s\n' "$*" >&2
  exit 1
}

normalize_version() {
  local raw="$1"
  [[ -n "$raw" ]] || die 'Usage: scripts/release-cut.sh 0.2.0 (or v0.2.0)'

  if [[ "$raw" == v* ]]; then
    VERSION="${raw#v}"
  else
    VERSION="$raw"
  fi

  if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    die "Version must be semver like 0.2.0 (or v0.2.0). Got: $raw"
  fi

  TAG="v$VERSION"
}

ensure_clean_worktree() {
  if ! git -C "$ROOT_DIR" diff --quiet || ! git -C "$ROOT_DIR" diff --cached --quiet; then
    die 'Working tree must be clean before cutting a release.'
  fi
}

ensure_branch() {
  local current_branch
  current_branch="$(git -C "$ROOT_DIR" rev-parse --abbrev-ref HEAD)"
  [[ "$current_branch" == "$RELEASE_BRANCH" ]] \
    || die "Release must run from branch '$RELEASE_BRANCH' (current: $current_branch)."
}

ensure_tag_absent() {
  if git -C "$ROOT_DIR" rev-parse --verify --quiet "refs/tags/$TAG" >/dev/null; then
    die "Local tag already exists: $TAG"
  fi

  if git -C "$ROOT_DIR" ls-remote --tags "$GIT_REMOTE" "refs/tags/$TAG" | grep -q .; then
    die "Remote tag already exists on $GIT_REMOTE: $TAG"
  fi
}

bump_workspace_version() {
  local cargo_toml="$ROOT_DIR/Cargo.toml"
  local tmp_file
  local old_version
  local old_version_file

  [[ -f "$cargo_toml" ]] || die "Missing Cargo.toml at $cargo_toml"

  old_version_file="$(mktemp)"
  old_version="$(
    awk -F'"' '
      /^\[workspace\.package\]/ { in_section=1; next }
      in_section && /^[[:space:]]*version[[:space:]]*=/ { print $2; exit }
      in_section && /^\[/ { in_section=0 }
    ' "$cargo_toml"
  )"
  [[ -n "$old_version" ]] || die "Unable to read workspace.package.version from $cargo_toml"

  if [[ "$old_version" == "$VERSION" ]]; then
    die "workspace.package.version is already $VERSION"
  fi
  printf '%s\n' "$old_version" > "$old_version_file"

  tmp_file="$(mktemp)"
  awk -v new_version="$VERSION" '
    BEGIN { in_section=0; updated=0 }
    /^\[workspace\.package\]/ { in_section=1; print; next }
    in_section && /^[[:space:]]*version[[:space:]]*=/ {
      sub(/"[^"]*"/, "\"" new_version "\"")
      in_section=0
      updated=1
      print
      next
    }
    in_section && /^\[/ { in_section=0 }
    { print }
    END {
      if (!updated) {
        exit 42
      }
    }
  ' "$cargo_toml" > "$tmp_file" || {
    rm -f "$tmp_file" "$old_version_file"
    die "Failed to update workspace.package.version in $cargo_toml"
  }

  mv "$tmp_file" "$cargo_toml"

  log "Updated workspace.package.version: $(cat "$old_version_file") -> $VERSION"
  rm -f "$old_version_file"
}

commit_and_push() {
  git -C "$ROOT_DIR" add Cargo.toml
  git -C "$ROOT_DIR" commit -m "chore(release): bump version to $VERSION"
  git -C "$ROOT_DIR" push "$GIT_REMOTE" "$RELEASE_BRANCH"
}

tag_and_push() {
  git -C "$ROOT_DIR" tag "$TAG"
  git -C "$ROOT_DIR" push "$GIT_REMOTE" "$TAG"
}

main() {
  normalize_version "$VERSION_INPUT"
  ensure_clean_worktree
  ensure_branch
  ensure_tag_absent
  bump_workspace_version
  commit_and_push
  tag_and_push
  log "Release cut complete: $TAG"
}

main "$@"
