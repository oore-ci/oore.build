#!/usr/bin/env bash
set -euo pipefail

# Move validated code onto a release branch without a person in the loop.
#
#   release-train.sh alpha          Ship master to alpha, bumping the
#                                   workspace version first when stable
#                                   already shipped it.
#   release-train.sh stable [tag]   Promote a published alpha (the newest
#                                   one by default) to stable.
#
# Every move is a pull request that must pass "Required validation" before
# the train merges it. The merge pushes the release branch, and autotag.yml
# then cuts the tag and starts release.yml exactly as a manual merge would.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

REQUIRED_CHECK="Required validation"
CHECK_TIMEOUT_SECONDS="${RELEASE_TRAIN_CHECK_TIMEOUT_SECONDS:-5400}"
CHECK_POLL_SECONDS="${RELEASE_TRAIN_CHECK_POLL_SECONDS:-30}"
DRY_RUN="${RELEASE_TRAIN_DRY_RUN:-false}"

# Logs go to stderr so functions can print their result on stdout.
log() {
  printf '[release-train] %s\n' "$*" >&2
}

fail() {
  printf '[release-train] %s\n' "$*" >&2
  exit 1
}

summary() {
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '%s\n' "$*" >>"$GITHUB_STEP_SUMMARY"
  fi
  log "$*"
}

semver_gt() {
  # Return 0 when $1 > $2, else 1.
  local a b c x y z
  IFS=. read -r a b c <<<"$1"
  IFS=. read -r x y z <<<"$2"
  if (( a != x )); then (( a > x )); return; fi
  if (( b != y )); then (( b > y )); return; fi
  (( c > z ))
}

workspace_version_at() {
  git show "$1:Cargo.toml" | awk '
    $0 == "[workspace.package]" { inside = 1; next }
    inside && /^\[/ { exit }
    inside && /^[[:space:]]*version[[:space:]]*=/ {
      value = $0
      sub(/^[^=]*=[[:space:]]*"/, "", value)
      sub(/"[[:space:]]*$/, "", value)
      print value
      exit
    }
  '
}

latest_stable_version() {
  local tag
  tag="$(git tag -l 'v[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname | grep -Ev -- '-' | head -n1 || true)"
  if [[ -z "$tag" ]]; then
    printf '0.0.0\n'
  else
    printf '%s\n' "${tag#v}"
  fi
}

next_prerelease_tag() {
  local version="$1" channel="$2" previous number
  previous="$(git tag -l "v$version-$channel.*" --sort=-v:refname | head -n1 || true)"
  number="${previous##*.}"
  [[ "$number" =~ ^[0-9]+$ ]] || number=0
  printf 'v%s-%s.%s\n' "$version" "$channel" "$((number + 1))"
}

fetch_refs() {
  git fetch --quiet --force origin \
    '+refs/heads/master:refs/remotes/origin/master' \
    '+refs/heads/alpha:refs/remotes/origin/alpha' \
    '+refs/heads/stable:refs/remotes/origin/stable' \
    '+refs/tags/*:refs/tags/*'
}

# Fail unless merging $2 into branch $1 yields exactly the tree of $2, so
# the release branch ships the validated source and nothing else.
require_clean_promotion() {
  local base="$1" head="$2" merged
  merged="$(git merge-tree --write-tree "origin/$base" "$head")" || \
    fail "Merging $head into $base conflicts. Resolve $base by hand."
  [[ "$merged" == "$(git rev-parse "$head^{tree}")" ]] || \
    fail "$base has changes that are not in $head. Resolve $base by hand."
}

open_pr_number() {
  gh pr list --state open --base "$1" --head "$2" --json number --jq '.[0].number // empty'
}

ensure_pr() {
  local base="$1" head="$2" title="$3" body="$4" number
  number="$(open_pr_number "$base" "$head")"
  if [[ -n "$number" ]]; then
    log "Reusing #$number ($head -> $base)."
  else
    gh pr create --base "$base" --head "$head" --title "$title" --body "$body" >/dev/null
    number="$(open_pr_number "$base" "$head")"
    [[ -n "$number" ]] || fail "GitHub did not return the new $head -> $base pull request."
    log "Opened #$number ($head -> $base)."
  fi
  printf '%s\n' "$number"
}

# Wait until the pull request head passes the required check, then print
# that head commit. Fails on a failed check or after the timeout.
wait_for_required_check() {
  local number="$1" deadline state head
  deadline="$((SECONDS + CHECK_TIMEOUT_SECONDS))"
  while (( SECONDS < deadline )); do
    if ! read -r head state < <(
      gh pr view "$number" --json headRefOid,statusCheckRollup --jq "
        [.statusCheckRollup[] | select(.name == \"$REQUIRED_CHECK\")] as \$checks
        | .headRefOid + \" \" + (
            if (\$checks | length) == 0 then \"pending\"
            elif any(\$checks[]; .status != \"COMPLETED\") then \"pending\"
            elif any(\$checks[]; .conclusion == \"SUCCESS\") then \"success\"
            else \"failure\" end)
      "
    ); then
      log "Could not read the checks on #$number. Retrying."
      sleep "$CHECK_POLL_SECONDS"
      continue
    fi
    case "$state" in
      success)
        printf '%s\n' "$head"
        return 0
        ;;
      failure)
        fail "#$number failed \"$REQUIRED_CHECK\" on $head."
        ;;
    esac
    sleep "$CHECK_POLL_SECONDS"
  done
  fail "#$number did not finish \"$REQUIRED_CHECK\" within $CHECK_TIMEOUT_SECONDS seconds."
}

merge_pr() {
  local number="$1" method="$2" head="$3"
  shift 3
  if [[ "$DRY_RUN" == true ]]; then
    summary "Dry run: would merge #$number at ${head:0:12}."
    return 0
  fi
  gh pr merge "$number" "--$method" --match-head-commit "$head" "$@"
  summary "Merged #$number at ${head:0:12}."
}

bump_master() {
  local version="$1" branch="release-train/bump-v$1" number head
  local title="chore(release): prepare v$version"

  git checkout --quiet --detach origin/master
  bash tools/bump-workspace-version.sh "$version"
  git -c user.name='github-actions[bot]' \
    -c user.email='41898282+github-actions[bot]@users.noreply.github.com' \
    commit --quiet -am "$title"
  if [[ "$DRY_RUN" == true ]]; then
    summary "Dry run: would open \"$title\" against master."
    return 0
  fi
  git push --quiet --force origin "HEAD:refs/heads/$branch"

  number="$(ensure_pr master "$branch" "$title" \
    "Stable already shipped the current workspace version, so the release train moves master to v$version before the next alpha.")"
  head="$(wait_for_required_check "$number")"
  merge_pr "$number" squash "$head" --delete-branch
  fetch_refs
}

train_alpha() {
  local stable version next_tag number head

  fetch_refs
  if [[ "$(git rev-parse 'origin/alpha^{tree}')" == "$(git rev-parse 'origin/master^{tree}')" ]]; then
    summary "alpha already ships master $(git rev-parse --short origin/master). Nothing to release."
    return 0
  fi

  stable="$(latest_stable_version)"
  version="$(workspace_version_at origin/master)"
  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "master has no valid workspace version."
  if ! semver_gt "$version" "$stable"; then
    local a b c
    IFS=. read -r a b c <<<"$stable"
    version="$a.$b.$((c + 1))"
    summary "Stable already shipped v$stable. Bumping master to v$version."
    bump_master "$version"
    [[ "$DRY_RUN" == true ]] || [[ "$(workspace_version_at origin/master)" == "$version" ]] || \
      fail "master did not reach v$version after the bump."
  fi

  require_clean_promotion alpha origin/master
  next_tag="$(next_prerelease_tag "$version" alpha)"
  if [[ "$DRY_RUN" == true ]]; then
    summary "Dry run: would release master $(git rev-parse --short origin/master) as $next_tag."
    return 0
  fi

  number="$(ensure_pr alpha master "Release $next_tag" \
    "The release train merges master into alpha once \"$REQUIRED_CHECK\" passes. Autotag then cuts $next_tag and starts the Release workflow.")"
  head="$(wait_for_required_check "$number")"
  merge_pr "$number" merge "$head"
  summary "alpha now ships ${head:0:12}. Autotag cuts $next_tag next."
}

latest_published_alpha() {
  gh api --paginate "repos/$GITHUB_REPOSITORY/releases?per_page=100" \
    --jq '.[] | select(.draft == false and .prerelease == true) | .tag_name' |
    grep -E '^v[0-9]+\.[0-9]+\.[0-9]+-alpha\.[0-9]+$' | sort -V | tail -n1
}

train_stable() {
  local tag="${1:-}" commit stable version branch number head release

  fetch_refs
  if [[ -z "$tag" ]]; then
    tag="$(latest_published_alpha || true)"
    [[ -n "$tag" ]] || fail "There is no published alpha release to promote."
  fi
  [[ "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)-alpha\.[0-9]+$ ]] || fail "Promote an alpha tag, not $tag."
  version="${BASH_REMATCH[1]}"

  release="$(gh api "repos/$GITHUB_REPOSITORY/releases/tags/$tag" \
    --jq '[.draft, .prerelease, .immutable] | map(tostring) | join(" ")')" || \
    fail "$tag has no GitHub release. Only a published alpha can go to stable."
  [[ "$release" == "false true true" ]] || \
    fail "$tag is not a published, immutable alpha release (draft prerelease immutable: $release)."

  commit="$(git rev-parse "refs/tags/$tag^{commit}")"
  git merge-base --is-ancestor "$commit" origin/alpha || fail "$tag is not on alpha."
  [[ "$(workspace_version_at "$commit")" == "$version" ]] || fail "$tag does not carry workspace version $version."

  stable="$(latest_stable_version)"
  if git merge-base --is-ancestor "$commit" origin/stable; then
    summary "stable already contains $tag. Nothing to promote."
    return 0
  fi
  semver_gt "$version" "$stable" || fail "Stable already shipped v$stable, so $tag cannot become v$version."

  require_clean_promotion stable "$commit"
  if [[ "$DRY_RUN" == true ]]; then
    summary "Dry run: would promote $tag (${commit:0:12}) to v$version."
    return 0
  fi

  branch="release-train/stable-$tag"
  git push --quiet --force origin "$commit:refs/heads/$branch"
  number="$(ensure_pr stable "$branch" "Release v$version" \
    "The release train promotes $tag to stable once \"$REQUIRED_CHECK\" passes. Autotag then cuts v$version from the same source and starts the Release workflow.")"
  head="$(wait_for_required_check "$number")"
  [[ "$head" == "$commit" ]] || fail "#$number moved off $tag."
  merge_pr "$number" merge "$head" --delete-branch
  summary "stable now ships $tag. Autotag cuts v$version next."
}

[[ -n "${GH_TOKEN:-}" ]] || fail "GH_TOKEN is required."
[[ -n "${GITHUB_REPOSITORY:-}" ]] || fail "GITHUB_REPOSITORY is required."

case "${1:-}" in
  alpha) train_alpha ;;
  stable) train_stable "${2:-}" ;;
  *)
    echo "Usage: $0 alpha | stable [vX.Y.Z-alpha.N]" >&2
    exit 2
    ;;
esac
