#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG_INPUT="${1:-${TAG:-}}"
GIT_REMOTE="${OORE_GIT_REMOTE:-origin}"
R2_BUCKET="${OORE_R2_BUCKET:-oore}"
R2_PREFIX="${OORE_R2_PREFIX:-releases}"
RELEASE_BASE_URL="${OORE_RELEASE_BASE_URL:-https://dl.oore.build/releases}"
RELEASE_DIST_ROOT="${OORE_RELEASE_DIST_ROOT:-$ROOT_DIR/dist/releases}"
PUBLISH_LATEST="${OORE_PUBLISH_LATEST:-1}"
SKIP_UPLOAD="${OORE_SKIP_UPLOAD:-0}"
BUN_VERSION="${OORE_BUN_VERSION:-}"

TMP_DIR=""
WORKTREE_DIR=""
RELEASE_TAG=""
RELEASE_VERSION=""

log() {
  printf '[release-local] %s\n' "$*"
}

die() {
  printf '[release-local] ERROR: %s\n' "$*" >&2
  exit 1
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

normalize_bool() {
  case "${1:-}" in
    1|true|TRUE|yes|YES|on|ON)
      return 0
      ;;
    0|false|FALSE|no|NO|off|OFF)
      return 1
      ;;
    *)
      return 2
      ;;
  esac
}

cleanup() {
  if [[ -n "$WORKTREE_DIR" && -d "$WORKTREE_DIR" ]]; then
    git -C "$ROOT_DIR" worktree remove --force "$WORKTREE_DIR" >/dev/null 2>&1 || true
  fi
  if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR"
  fi
}

ensure_dependencies() {
  local dep
  for dep in git cargo tar shasum mktemp date bun curl unzip; do
    have_cmd "$dep" || die "$dep is required."
  done

  if normalize_bool "$SKIP_UPLOAD"; then
    :
  else
    if [[ "$?" -eq 2 ]]; then
      die 'OORE_SKIP_UPLOAD must be one of: true,false,1,0,yes,no,on,off.'
    fi
    have_cmd wrangler || die 'wrangler is required for R2 upload. Install with `npm i -g wrangler`.'
  fi
}

resolve_bun_version() {
  if [[ -n "$BUN_VERSION" ]]; then
    if [[ "$BUN_VERSION" == v* ]]; then
      printf '%s' "$BUN_VERSION"
    else
      printf 'v%s' "$BUN_VERSION"
    fi
    return 0
  fi

  printf 'v%s' "$(bun --version)"
}

download_bun_runtime() {
  local bun_version="$1"
  local release_arch="$2"
  local bun_arch=""
  local runtime_root="$TMP_DIR/bun-runtime"
  local runtime_dir=""
  local zip_name=""
  local zip_path=""
  local url=""

  case "$release_arch" in
    arm64) bun_arch="aarch64" ;;
    x86_64) bun_arch="x64" ;;
    *)
      die "Unsupported release architecture for bun runtime: $release_arch"
      ;;
  esac

  runtime_dir="$runtime_root/bun-darwin-$bun_arch"
  if [[ -x "$runtime_dir/bun" ]]; then
    printf '%s' "$runtime_dir/bun"
    return 0
  fi

  mkdir -p "$runtime_root"
  zip_name="bun-darwin-$bun_arch.zip"
  zip_path="$runtime_root/$zip_name"
  url="https://github.com/oven-sh/bun/releases/download/bun-$bun_version/$zip_name"

  # This function is used via command substitution, so progress logs must go to stderr.
  printf '[release-local] %s\n' "Downloading bun runtime $bun_version ($release_arch)..." >&2
  curl -fsSL --retry 3 --output "$zip_path" "$url" \
    || die "Failed to download bun runtime: $url"

  unzip -q "$zip_path" -d "$runtime_root" \
    || die "Failed to extract bun runtime archive: $zip_path"

  [[ -x "$runtime_dir/bun" ]] || die "bun runtime archive missing binary: $runtime_dir/bun"
  printf '%s' "$runtime_dir/bun"
}

normalize_tag() {
  local input="$1"
  [[ -n "$input" ]] || die 'Usage: scripts/release-local.sh vX.Y.Z'

  if [[ "$input" == v* ]]; then
    RELEASE_TAG="$input"
  else
    RELEASE_TAG="v$input"
  fi

  RELEASE_VERSION="${RELEASE_TAG#v}"
  [[ -n "$RELEASE_VERSION" ]] || die "Invalid release tag: $RELEASE_TAG"
}

prepare_worktree() {
  # Force-refresh tags so stale local tags do not block release automation.
  git -C "$ROOT_DIR" fetch "$GIT_REMOTE" --tags --force

  if ! git -C "$ROOT_DIR" rev-parse --verify --quiet "refs/tags/$RELEASE_TAG" >/dev/null; then
    die "Tag not found: $RELEASE_TAG"
  fi

  TMP_DIR="$(mktemp -d)"
  WORKTREE_DIR="$TMP_DIR/src"
  git -C "$ROOT_DIR" worktree add --detach "$WORKTREE_DIR" "$RELEASE_TAG" >/dev/null
}

ensure_tag_matches_workspace_version() {
  local cargo_toml="$WORKTREE_DIR/Cargo.toml"
  local workspace_version=""

  [[ -f "$cargo_toml" ]] || die "Missing workspace Cargo.toml in release source: $cargo_toml"

  workspace_version="$(
    awk -F'"' '
      /^\[workspace\.package\]/ { in_section=1; next }
      in_section && /^[[:space:]]*version[[:space:]]*=/ { print $2; exit }
      in_section && /^\[/ { in_section=0 }
    ' "$cargo_toml"
  )"

  [[ -n "$workspace_version" ]] || die "Unable to read workspace.package.version from $cargo_toml"

  if [[ "$workspace_version" != "$RELEASE_VERSION" ]]; then
    die "Tag/version mismatch: tag=$RELEASE_TAG expects version=$RELEASE_VERSION but workspace.package.version=$workspace_version. Update Cargo.toml version or retag."
  fi

  log "Version check passed: $RELEASE_TAG matches workspace.package.version=$workspace_version"
}

build_binaries() {
  log "Building release binaries for $RELEASE_TAG..."
  (
    cd "$WORKTREE_DIR"
    cargo build --release -p oored -p oore
    cargo build --release --target x86_64-apple-darwin -p oored -p oore
  )
}

build_web_launcher() {
  local bun_version=""
  local bun_arm64=""
  local bun_x64=""
  local entrypoint="$WORKTREE_DIR/apps/web/tools/oore-web.js"
  local dist_dir="$WORKTREE_DIR/apps/web/dist"

  [[ -f "$entrypoint" ]] || die "Missing web launcher entrypoint: $entrypoint"

  log "Building web UI assets..."
  (
    cd "$WORKTREE_DIR"
    bun install --frozen-lockfile
    bun run build:web
  )

  [[ -f "$dist_dir/index.html" ]] || die "Missing built web assets: $dist_dir/index.html"

  bun_version="$(resolve_bun_version)"
  bun_arm64="$(download_bun_runtime "$bun_version" "arm64")"
  bun_x64="$(download_bun_runtime "$bun_version" "x86_64")"

  log "Compiling oore-web launcher binaries..."
  bun build --compile \
    --target=bun \
    --compile-executable-path "$bun_arm64" \
    --outfile "$TMP_DIR/oore-web-arm64" \
    "$entrypoint"

  bun build --compile \
    --target=bun \
    --compile-executable-path "$bun_x64" \
    --outfile "$TMP_DIR/oore-web-x86_64" \
    "$entrypoint"

  chmod +x "$TMP_DIR/oore-web-arm64" "$TMP_DIR/oore-web-x86_64"
}

package_release() {
  local out_dir="$RELEASE_DIST_ROOT/$RELEASE_TAG"
  local arm_asset="oore_${RELEASE_VERSION}_darwin_arm64.tar.gz"
  local x64_asset="oore_${RELEASE_VERSION}_darwin_x86_64.tar.gz"
  local checksum_asset="oore_${RELEASE_VERSION}_checksums.txt"
  local released_at
  local base_url="${RELEASE_BASE_URL%/}/$RELEASE_TAG"

  local arm_stage="$TMP_DIR/stage-arm64"
  local x64_stage="$TMP_DIR/stage-x86_64"
  mkdir -p "$arm_stage/bin" "$x64_stage/bin" "$out_dir"

  cp "$WORKTREE_DIR/target/release/oored" "$arm_stage/bin/oored"
  cp "$WORKTREE_DIR/target/release/oore" "$arm_stage/bin/oore"
  cp "$TMP_DIR/oore-web-arm64" "$arm_stage/bin/oore-web"
  cp -R "$WORKTREE_DIR/apps/web/dist" "$arm_stage/web-dist"
  cp "$WORKTREE_DIR/LICENSE" "$arm_stage/LICENSE"
  printf '%s\n' "$RELEASE_VERSION" > "$arm_stage/VERSION"

  cp "$WORKTREE_DIR/target/x86_64-apple-darwin/release/oored" "$x64_stage/bin/oored"
  cp "$WORKTREE_DIR/target/x86_64-apple-darwin/release/oore" "$x64_stage/bin/oore"
  cp "$TMP_DIR/oore-web-x86_64" "$x64_stage/bin/oore-web"
  cp -R "$WORKTREE_DIR/apps/web/dist" "$x64_stage/web-dist"
  cp "$WORKTREE_DIR/LICENSE" "$x64_stage/LICENSE"
  printf '%s\n' "$RELEASE_VERSION" > "$x64_stage/VERSION"

  tar -C "$arm_stage" -czf "$out_dir/$arm_asset" .
  tar -C "$x64_stage" -czf "$out_dir/$x64_asset" .

  (
    cd "$out_dir"
    shasum -a 256 "$arm_asset" "$x64_asset" > "$checksum_asset"
  )

  if ! awk -v file="$arm_asset" '$2 == file { found=1 } END { exit(found ? 0 : 1) }' "$out_dir/$checksum_asset"; then
    die "Missing checksum entry for $arm_asset"
  fi
  if ! awk -v file="$x64_asset" '$2 == file { found=1 } END { exit(found ? 0 : 1) }' "$out_dir/$checksum_asset"; then
    die "Missing checksum entry for $x64_asset"
  fi

  released_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  cat > "$out_dir/manifest.json" <<EOF
{
  "tag": "$RELEASE_TAG",
  "version": "$RELEASE_VERSION",
  "released_at": "$released_at",
  "assets": {
    "darwin_arm64": "$base_url/$arm_asset",
    "darwin_x86_64": "$base_url/$x64_asset",
    "checksums": "$base_url/$checksum_asset"
  }
}
EOF

  cp "$out_dir/manifest.json" "$RELEASE_DIST_ROOT/latest.json"
  log "Release artifacts written to $out_dir"
}

upload_to_r2() {
  local out_dir="$RELEASE_DIST_ROOT/$RELEASE_TAG"
  local arm_asset="oore_${RELEASE_VERSION}_darwin_arm64.tar.gz"
  local x64_asset="oore_${RELEASE_VERSION}_darwin_x86_64.tar.gz"
  local checksum_asset="oore_${RELEASE_VERSION}_checksums.txt"

  log "Uploading artifacts to r2://$R2_BUCKET/$R2_PREFIX/$RELEASE_TAG/"

  wrangler r2 object put "$R2_BUCKET/$R2_PREFIX/$RELEASE_TAG/$arm_asset" \
    --remote \
    --file "$out_dir/$arm_asset" \
    --content-type "application/gzip" \
    --cache-control "public, max-age=31536000, immutable"

  wrangler r2 object put "$R2_BUCKET/$R2_PREFIX/$RELEASE_TAG/$x64_asset" \
    --remote \
    --file "$out_dir/$x64_asset" \
    --content-type "application/gzip" \
    --cache-control "public, max-age=31536000, immutable"

  wrangler r2 object put "$R2_BUCKET/$R2_PREFIX/$RELEASE_TAG/$checksum_asset" \
    --remote \
    --file "$out_dir/$checksum_asset" \
    --content-type "text/plain; charset=utf-8" \
    --cache-control "public, max-age=31536000, immutable"

  wrangler r2 object put "$R2_BUCKET/$R2_PREFIX/$RELEASE_TAG/manifest.json" \
    --remote \
    --file "$out_dir/manifest.json" \
    --content-type "application/json; charset=utf-8" \
    --cache-control "public, max-age=300, must-revalidate"

  if normalize_bool "$PUBLISH_LATEST"; then
    wrangler r2 object put "$R2_BUCKET/$R2_PREFIX/latest.json" \
      --remote \
      --file "$RELEASE_DIST_ROOT/latest.json" \
      --content-type "application/json; charset=utf-8" \
      --cache-control "public, max-age=60, must-revalidate"
  else
    if [[ "$?" -eq 2 ]]; then
      die 'OORE_PUBLISH_LATEST must be one of: true,false,1,0,yes,no,on,off.'
    fi
  fi
}

main() {
  trap cleanup EXIT

  [[ "$(uname -s)" == "Darwin" ]] || die 'This release script must run on macOS.'

  ensure_dependencies
  normalize_tag "$TAG_INPUT"
  prepare_worktree
  ensure_tag_matches_workspace_version
  build_binaries
  build_web_launcher
  package_release

  if normalize_bool "$SKIP_UPLOAD"; then
    log 'Skipping R2 upload (OORE_SKIP_UPLOAD=true).'
  else
    upload_to_r2
    log "Uploaded release for $RELEASE_TAG to https://dl.oore.build/releases/$RELEASE_TAG/"
  fi
}

main "$@"
