#!/usr/bin/env bash
set -euo pipefail

# Refuse to publish a release that a newer release has already superseded.
# A rerun of an older release run would otherwise deploy its older signed
# index, installer, or Pages build over the newer one.
#
# Usage: require-current-release.sh <tag> [--most-recent] [--index <dir>]
#
# Always: no published release in the tag's channel is newer than the tag.
# --most-recent: the tag is also the most recently published release in any
#   channel. Use it for outputs every channel shares, like the installer.
# --index <dir>: every channel's latest/<channel>.json in the built index
#   names the newest published release of that channel.
#
# Reads published releases with gh. Set GH_RELEASES_JSON to a slurped
# `gh api --paginate --slurp` releases listing to use a saved copy instead.

usage() {
  echo "Usage: $0 <tag> [--most-recent] [--index <dir>]" >&2
  exit 2
}

[[ "$#" -ge 1 ]] || usage
tag="$1"
shift
most_recent=false
index_dir=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --most-recent) most_recent=true ;;
    --index)
      [[ "$#" -ge 2 ]] || usage
      index_dir="$2"
      shift
      ;;
    *) usage ;;
  esac
  shift
done

channel_of() {
  if [[ "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo stable
  elif [[ "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-alpha\.[0-9]+$ ]]; then
    echo alpha
  elif [[ "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-beta\.[0-9]+$ ]]; then
    echo beta
  else
    return 1
  fi
}

channel="$(channel_of "$tag")" || {
  echo "Unsupported release tag: $tag" >&2
  exit 1
}

# Print "<tag>\t<published_at>" for every published release with a release tag.
published_releases() {
  if [[ -n "${GH_RELEASES_JSON:-}" ]]; then
    cat "$GH_RELEASES_JSON"
  else
    gh api --paginate "repos/$GITHUB_REPOSITORY/releases?per_page=100" --slurp
  fi | jq -r '.[][] | select(.draft == false) | [.tag_name, (.published_at // .created_at)] | @tsv'
}

releases="$(published_releases)"

# Newest published tag in a channel, or empty when the channel has none.
newest_in_channel() {
  local wanted="$1" candidate _published
  while IFS=$'\t' read -r candidate _published; do
    [[ -n "$candidate" ]] || continue
    if [[ "$(channel_of "$candidate" || true)" == "$wanted" ]]; then
      printf '%s\n' "$candidate"
    fi
  done <<<"$releases" | sort -V | tail -n1
}

newest="$(newest_in_channel "$channel")"
if [[ -n "$newest" && "$newest" != "$tag" ]]; then
  if [[ "$(printf '%s\n%s\n' "$newest" "$tag" | sort -V | tail -n1)" != "$tag" ]]; then
    echo "$tag is older than published $newest for $channel. Dispatch release.yml for $newest instead of rerunning this run." >&2
    exit 1
  fi
fi

if [[ "$most_recent" == true ]]; then
  latest="$(
    while IFS=$'\t' read -r candidate published; do
      [[ -n "$candidate" ]] || continue
      channel_of "$candidate" >/dev/null || continue
      printf '%s\t%s\n' "$published" "$candidate"
    done <<<"$releases" | LC_ALL=C sort | tail -n1 | cut -f2
  )"
  [[ -z "$latest" || "$latest" == "$tag" ]] || {
    echo "$latest was published after $tag. Dispatch release.yml for $latest instead of rerunning this run." >&2
    exit 1
  }
fi

if [[ -n "$index_dir" ]]; then
  for index_channel in alpha beta stable; do
    expected="$(newest_in_channel "$index_channel")"
    manifest="$index_dir/latest/$index_channel.json"
    actual=""
    if [[ -e "$manifest" ]]; then
      actual="$(jq -r '.tag' "$manifest")"
    fi
    [[ "$actual" == "$expected" ]] || {
      echo "The built index names ${actual:-no release} as the latest $index_channel release, but ${expected:-none} is published. Dispatch release.yml for the newest tag instead of rerunning this run." >&2
      exit 1
    }
  done
fi

echo "$tag is the current $channel release."
