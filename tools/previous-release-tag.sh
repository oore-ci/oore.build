#!/usr/bin/env bash
# Print the closest tag below TAG from the tags given on stdin, one per line.
#
# Release tags can be cut before earlier releases finish publishing (a queued
# release train), so the newest tag is not always older than TAG. Release
# notes must compare against the release just before TAG instead.
set -euo pipefail

if [[ $# -ne 1 || -z "$1" ]]; then
  echo "usage: $0 TAG < tags" >&2
  exit 2
fi
tag="$1"

{ grep -v -x -F -- "$tag" || true; } |
  { grep -v '^$' || true; } |
  { printf '%s\n' "$tag"; cat; } |
  sort -u -V |
  awk -v tag="$tag" '$0 == tag { print prev; exit } { prev = $0 }'
