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

read_workspace_version() {
  awk -F'"' '
    /^\[workspace\.package\]/ { in_section=1; next }
    in_section && /^[[:space:]]*version[[:space:]]*=/ { print $2; exit }
    in_section && /^\[/ { in_section=0 }
  ' Cargo.toml
}

summary() {
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '%s\n' "$*" >>"$GITHUB_STEP_SUMMARY"
  fi
  echo "$*"
}

emit_tag() {
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    printf 'tag=%s\n' "$1" >>"$GITHUB_OUTPUT"
  fi
}

# Annotated tags need an identity; hosted Linux runners have none.
tag_release() {
  git -c user.name='github-actions[bot]' \
    -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
    tag -a "$1" -m "$2" "$TARGET_SHA"
  git push origin "$1"
  summary "[autotag:$CHANNEL] cut $1 at ${TARGET_SHA:0:12}."
  emit_tag "$1"
}

existing_tag_for_commit() {
  local pattern="$1"
  local candidate=""
  local candidate_commit=""
  local match=""
  local count=0
  while IFS= read -r candidate; do
    [[ -n "$candidate" ]] || continue
    candidate_commit="$(git rev-parse "refs/tags/$candidate^{commit}")"
    if [[ "$candidate_commit" == "$TARGET_SHA" ]]; then
      match="$candidate"
      count="$((count + 1))"
    fi
  done < <(git tag -l "$pattern")
  if (( count > 1 )); then
    echo "Multiple release tags for $TARGET_SHA match $pattern." >&2
    return 1
  fi
  printf '%s\n' "$match"
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
  summary "[autotag:$CHANNEL] skipped a CI-generated commit. No tag."
  exit 0
fi

git fetch origin "+refs/heads/$BRANCH:refs/remotes/origin/$BRANCH" "+refs/tags/*:refs/tags/*"
if [[ -n "$SHA" ]]; then
  git merge-base --is-ancestor "$SHA" "origin/$BRANCH" \
    || { echo "Validated commit $SHA is not on origin/$BRANCH" >&2; exit 1; }
  git checkout --detach "$SHA"
else
  git checkout -B "$BRANCH" "origin/$BRANCH"
fi
TARGET_SHA="$(git rev-parse HEAD)"

cargo_ver="$(read_workspace_version)"
if [[ ! "$cargo_ver" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Failed to read a valid workspace.package.version from Cargo.toml" >&2
  exit 1
fi

latest_prod_ver="$(latest_prod_version)"

if [[ "$CHANNEL" == stable ]]; then
  replay_tag="$(existing_tag_for_commit "v$cargo_ver")"
else
  replay_tag="$(existing_tag_for_commit "v$cargo_ver-$CHANNEL.*")"
fi
if [[ -n "$replay_tag" ]]; then
  summary "[autotag:$CHANNEL] $replay_tag already tags ${TARGET_SHA:0:12}. Starting its release again."
  emit_tag "$replay_tag"
  exit 0
fi

if ! semver_gt "$cargo_ver" "$latest_prod_ver"; then
  # A release branch push that cuts no tag is a release that silently did
  # not happen. Fail so it shows up.
  summary "[autotag:$CHANNEL] no tag: workspace version $cargo_ver has not advanced beyond stable $latest_prod_ver. Bump master with \`make bump-version\` (the Release train does this) and merge again."
  exit 1
fi

if [[ "$CHANNEL" == "stable" ]]; then
  tag="v$cargo_ver"
  if git show-ref --tags --quiet "refs/tags/$tag"; then
    echo "[autotag:stable] $tag exists on a different commit." >&2
    exit 1
  fi
  tag_release "$tag" "Release $tag"
  exit 0
fi

# alpha/beta
base="$cargo_ver"

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
if git show-ref --tags --quiet "refs/tags/$tag"; then
  echo "[autotag:$CHANNEL] $tag already exists on a different commit." >&2
  exit 1
fi
if [[ "$CHANNEL" == "alpha" ]]; then
  label="Alpha"
else
  label="Beta"
fi
tag_release "$tag" "$label $tag"
