#!/usr/bin/env bash
set -euo pipefail

APP_DIR="${1:-apps/web}"
DIST_DIR="${APP_DIR%/}/dist"
ASSETS_DIR="$DIST_DIR/assets"
INDEX_HTML="$DIST_DIR/index.html"

MAX_TOTAL_GZIP_KB="${WEB_BUNDLE_MAX_TOTAL_GZIP_KB:-220}"
MAX_INITIAL_GZIP_KB="${WEB_BUNDLE_MAX_INITIAL_GZIP_KB:-120}"

MAX_TOTAL_GZIP_BYTES=$((MAX_TOTAL_GZIP_KB * 1024))
MAX_INITIAL_GZIP_BYTES=$((MAX_INITIAL_GZIP_KB * 1024))

if [[ ! -d "$ASSETS_DIR" ]]; then
  echo "[bundle-budget] Missing assets directory: $ASSETS_DIR" >&2
  exit 1
fi

if [[ ! -f "$INDEX_HTML" ]]; then
  echo "[bundle-budget] Missing index.html: $INDEX_HTML" >&2
  exit 1
fi

total_gzip_bytes=0
js_files=("$ASSETS_DIR"/*.js)
if [[ ${#js_files[@]} -eq 0 ]]; then
  echo "[bundle-budget] No JS assets found in $ASSETS_DIR" >&2
  exit 1
fi

for file in "${js_files[@]}"; do
  if [[ -f "$file" ]]; then
    gzip_bytes=$(gzip -c "$file" | wc -c | tr -d ' ')
    total_gzip_bytes=$((total_gzip_bytes + gzip_bytes))
  fi
done

initial_js_paths=$(grep -Eo '(href|src)="/assets/[^"]+\.js"' "$INDEX_HTML" | sed -E 's/^(href|src)="\/assets\/(.*)"$/\2/' | sort -u)

largest_initial_gzip_bytes=0
largest_initial_name=""

while IFS= read -r relative; do
  [[ -z "$relative" ]] && continue
  file="$ASSETS_DIR/$relative"
  if [[ ! -f "$file" ]]; then
    continue
  fi

  gzip_bytes=$(gzip -c "$file" | wc -c | tr -d ' ')
  if [[ "$gzip_bytes" -gt "$largest_initial_gzip_bytes" ]]; then
    largest_initial_gzip_bytes="$gzip_bytes"
    largest_initial_name="$relative"
  fi
done <<< "$initial_js_paths"

if [[ -z "$largest_initial_name" ]]; then
  echo "[bundle-budget] Unable to determine initial JS chunk(s) from $INDEX_HTML" >&2
  exit 1
fi

echo "[bundle-budget] App: $APP_DIR"
echo "[bundle-budget] Total gzip JS: ${total_gzip_bytes} bytes (limit ${MAX_TOTAL_GZIP_BYTES})"
echo "[bundle-budget] Largest initial gzip JS: ${largest_initial_gzip_bytes} bytes (${largest_initial_name}) (limit ${MAX_INITIAL_GZIP_BYTES})"

if [[ "$total_gzip_bytes" -gt "$MAX_TOTAL_GZIP_BYTES" ]]; then
  echo "[bundle-budget] FAIL: Total gzip JS exceeds limit" >&2
  exit 1
fi

if [[ "$largest_initial_gzip_bytes" -gt "$MAX_INITIAL_GZIP_BYTES" ]]; then
  echo "[bundle-budget] FAIL: Largest initial chunk exceeds limit" >&2
  exit 1
fi

echo "[bundle-budget] PASS"
