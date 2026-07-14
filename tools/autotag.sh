#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CHANNEL="${AUTOTAG_CHANNEL:-}"
BRANCH="${AUTOTAG_BRANCH:-$CHANNEL}"
SHA="${AUTOTAG_SHA:-}"

if [[ -z "$CHANNEL" ]]; then
  echo "AUTOTAG_CHANNEL is required (stable|alpha|beta)." >&2
  exit 1
fi

case "$CHANNEL" in
  stable|alpha|beta) ;;
  *)
    echo "Unsupported AUTOTAG_CHANNEL=$CHANNEL (expected stable|alpha|beta)." >&2
    exit 1
    ;;
esac

semver_gt() {
  # Return 0 when $1 > $2, else 1.
  local v1="$1"
  local v2="$2"
  [[ -n "$v1" ]] || v1="0.0.0"
  [[ -n "$v2" ]] || v2="0.0.0"

  local a b c x y z
  IFS=. read -r a b c <<<"$v1"
  IFS=. read -r x y z <<<"$v2"

  [[ -n "$a" ]] || a=0
  [[ -n "$b" ]] || b=0
  [[ -n "$c" ]] || c=0
  [[ -n "$x" ]] || x=0
  [[ -n "$y" ]] || y=0
  [[ -n "$z" ]] || z=0

  if (( a > x )); then return 0; fi
  if (( a < x )); then return 1; fi
  if (( b > y )); then return 0; fi
  if (( b < y )); then return 1; fi
  if (( c > z )); then return 0; fi
  return 1
}

bump_patch() {
  local v="$1"
  [[ -n "$v" ]] || v="0.0.0"

  local a b c
  IFS=. read -r a b c <<<"$v"
  [[ -n "$a" ]] || a=0
  [[ -n "$b" ]] || b=0
  [[ -n "$c" ]] || c=0
  printf '%s.%s.%s\n' "$a" "$b" "$((c + 1))"
}

read_workspace_version() {
  awk -F'"' '
    /^\[workspace\.package\]/ { in_section=1; next }
    in_section && /^[[:space:]]*version[[:space:]]*=/ { print $2; exit }
    in_section && /^\[/ { in_section=0 }
  ' Cargo.toml
}

maybe_configure_github_token_remote() {
  local token
  token="$(printenv GITHUB_TOKEN || true)"
  if [[ -n "$token" ]]; then
    git config url."https://x-access-token:$token@github.com/".insteadOf "https://github.com/"
  fi
}

emit_tag() {
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    printf 'tag=%s\n' "$1" >>"$GITHUB_OUTPUT"
  fi
}

latest_prod_version() {
  local latest_prod_tag latest_prod_ver
  latest_prod_tag="$(git tag -l 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname | grep -v -- '-' | head -n1 || true)"
  latest_prod_ver="${latest_prod_tag#v}"
  if [[ -z "$latest_prod_tag" ]]; then
    latest_prod_ver="0.0.0"
  fi
  printf '%s\n' "$latest_prod_ver"
}

message="$(git log -1 --pretty=%B || true)"
if echo "$message" | grep -q "\\[CI SKIP\\]"; then
  echo "[autotag:$CHANNEL] skipping CI-generated commit."
  exit 0
fi

if [[ "$CHANNEL" == "stable" ]] && echo "$message" | grep -q "^chore(release): bump version to "; then
  echo "[autotag:stable] skipping release bump commit."
  exit 0
fi

maybe_configure_github_token_remote

git fetch origin "+refs/heads/$BRANCH:refs/remotes/origin/$BRANCH" "+refs/tags/*:refs/tags/*"
if [[ -n "$SHA" ]]; then
  git merge-base --is-ancestor "$SHA" "origin/$BRANCH" \
    || { echo "Validated commit $SHA is not on origin/$BRANCH" >&2; exit 1; }
  git checkout --detach "$SHA"
else
  git checkout -B "$BRANCH" "origin/$BRANCH"
fi

cargo_ver="$(read_workspace_version)"
if [[ -z "$cargo_ver" ]]; then
  echo "Failed to read workspace.package.version from Cargo.toml" >&2
  exit 1
fi

latest_prod_ver="$(latest_prod_version)"

if [[ "$CHANNEL" == "stable" ]]; then
  base="$latest_prod_ver"
  if semver_gt "$cargo_ver" "$latest_prod_ver"; then
    base="$cargo_ver"
  else
    base="$(bump_patch "$latest_prod_ver")"
  fi

  next="$base"
  while git show-ref --tags --quiet "refs/tags/v$next"; do
    next="$(bump_patch "$next")"
  done

  tag="v$next"
  echo "[autotag:stable] cutting $tag"
  git tag -a "$tag" -m "Release $tag"
  git push origin "$tag"
  emit_tag "$tag"
  exit 0
fi

# alpha/beta
base="$latest_prod_ver"
if semver_gt "$cargo_ver" "$latest_prod_ver"; then
  base="$cargo_ver"
else
  base="$(bump_patch "$latest_prod_ver")"
fi

prev="$(git tag -l "v$base-$CHANNEL.*" --sort=-v:refname | head -n1 || true)"
if [[ -n "$prev" ]]; then
  n="$(echo "$prev" | awk -F. '{print $NF}')"
  case "$n" in
    ''|*[!0-9]*) n=0 ;;
  esac
  next_n="$((n + 1))"
else
  next_n="1"
fi

tag="v${base}-${CHANNEL}.${next_n}"
if [[ "$CHANNEL" == "alpha" ]]; then
  label="Alpha"
else
  label="Beta"
fi
echo "[autotag:$CHANNEL] cutting $tag"
git tag -a "$tag" -m "$label $tag"
git push origin "$tag"
emit_tag "$tag"
