#!/usr/bin/env bash
set -euo pipefail

OORE_VERSION="${OORE_VERSION:-latest}"
OORE_CHANNEL="${OORE_CHANNEL:-stable}"
OORE_INSTALL_ROOT="${OORE_INSTALL_ROOT:-${HOME:?HOME is required}/.oore}"
OORE_MODIFY_PATH="${OORE_MODIFY_PATH:-auto}"
OORE_GITHUB_REPO="${OORE_GITHUB_REPO:-oore-ci/oore.build}"
OORE_RELEASE_BASE_URL="${OORE_RELEASE_BASE_URL:-https://github.com/$OORE_GITHUB_REPO/releases/download}"
OORE_RELEASE_INDEX_BASE_URL="${OORE_RELEASE_INDEX_BASE_URL:-https://releases.oore.build}"
OORE_RELEASE_MANIFEST_URL="${OORE_RELEASE_MANIFEST_URL:-}"
OORE_LEGACY_UPGRADE="${OORE_LEGACY_UPGRADE:-}"
OORE_ALLOW_UNSIGNED_LOCAL_RELEASE="${OORE_ALLOW_UNSIGNED_LOCAL_RELEASE:-}"

# tools/sync-install-scripts.sh replaces this token in the public installer.
OORE_RELEASE_SIGNING_PUBLIC_KEY='@OORE_RELEASE_SIGNING_PUBLIC_KEY@'
OORE_RELEASE_SIGNER_IDENTITY='release@oore.build'
OORE_RELEASE_MANIFEST_NAMESPACE='oore-release-manifest@oore.build'
OORE_RELEASE_INDEX_NAMESPACE='oore-release-index@oore.build'

RELEASE_OS=""
RELEASE_ARCH=""
RELEASE_TAG=""
RELEASE_VERSION=""
RESOLVED_CHANNEL=""
ARCHIVE_NAME=""
CLI_ARCHIVE_NAME=""
FULL_ARCHIVE_NAME=""
CHECKSUM_NAME=""
ARCHIVE_SHA256=""
MANIFEST_SHA256=""
TMP_DIR=""
SHELL_RC=""
SHELL_PATH_FILE=""
PATH_EXPORT_LINE=""
PATH_ACTION="none"
PATH_UPDATED=0
PATH_RC_MUTATION_STARTED=0
PATH_RC_EXISTED=0
PATH_RC_SNAPSHOT=""
SNAPSHOT_DIR=""
INSTALL_TRANSACTION_ACTIVE=0
ACTIVE_STAGED_FILE=""
INSTALL_ROOT_EXISTED=0
INSTALL_ROOT_ORIGINAL_MODE=""
BIN_DIR_EXISTED=0
BIN_DIR_ORIGINAL_MODE=""
GUIDED_INSTALL_SUPPORTED=0
LIFECYCLE_LOCK_PATH=""
LIFECYCLE_LOCK_HOLDER_PID=""
GUARD_FILE_IDENTITY=""
CLI_PUBLICATION_SKIPPED=0
CLI_CANDIDATE=""
BOOTSTRAP_ACTION=""
PRESERVED_PROFILE=""

# macOS ships BSD stat, lockf and shasum. Homebrew coreutils can put GNU tools
# with incompatible flags first in PATH, so call the system copies directly.
BSD_STAT=/usr/bin/stat
BSD_LOCKF=/usr/bin/lockf
BSD_SHASUM=/usr/bin/shasum
FILE_IDENTITY_FORMAT='%d:%i:%u:%Lp:%l:%z:%m:%c'

PATH_BLOCK_START="# >>> oore PATH >>>"
PATH_BLOCK_END="# <<< oore PATH <<<"
BOOTSTRAP_PATHS=(
  "bin/oore"
  "VERSION"
  "CHANNEL"
  "GITHUB_REPO"
  "BOOTSTRAP_ARCHIVE"
  "BOOTSTRAP_SHA256"
  "BOOTSTRAP_MANIFEST_SHA256"
  "SHELL_PATH_FILE"
)

usage() {
  cat <<'EOF'
Install the Oore CLI.

Usage:
  install.sh [options]

Options:
  --version <version>      Install a release tag or version instead of latest.
  --channel <channel>      Resolve latest from stable, beta, or alpha.
  --install-root <path>    Install under this directory. Default: ~/.oore
  --modify-path            Add the Oore bin directory to the shell PATH.
  --no-modify-path         Do not change shell configuration.
  -h, --help               Show this help.

Environment variables:
  OORE_VERSION
  OORE_CHANNEL
  OORE_INSTALL_ROOT
  OORE_MODIFY_PATH         auto, true, or false
  OORE_GITHUB_REPO
  OORE_RELEASE_BASE_URL
  OORE_RELEASE_INDEX_BASE_URL
  OORE_RELEASE_MANIFEST_URL
  OORE_LEGACY_UPGRADE      true permits confirmed legacy removal without a terminal.
  OORE_ALLOW_UNSIGNED_LOCAL_RELEASE
                           true permits an unsigned exact release from literal loopback for acceptance.
EOF
}

die() {
  printf 'Oore install failed: %s\n' "$*" >&2
  exit 1
}

have_command() {
  command -v "$1" >/dev/null 2>&1
}

cleanup() {
  local status=$?
  trap - EXIT
  set +e
  if [[ "$INSTALL_TRANSACTION_ACTIVE" -eq 1 ]]; then
    if ! rollback_install; then
      printf 'Oore install failed: the previous bootstrap state could not be fully restored.\n' >&2
    fi
  fi
  if [[ -n "$LIFECYCLE_LOCK_HOLDER_PID" ]]; then
    kill "$LIFECYCLE_LOCK_HOLDER_PID" 2>/dev/null
  fi
  if [[ -n "$TMP_DIR" && -d "$TMP_DIR" ]]; then
    rm -rf "$TMP_DIR"
  fi
  exit "$status"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version)
        [[ $# -ge 2 ]] || die '--version requires a value.'
        OORE_VERSION="$2"
        shift 2
        ;;
      --channel)
        [[ $# -ge 2 ]] || die '--channel requires a value.'
        OORE_CHANNEL="$2"
        shift 2
        ;;
      --install-root)
        [[ $# -ge 2 ]] || die '--install-root requires a value.'
        OORE_INSTALL_ROOT="$2"
        shift 2
        ;;
      --modify-path)
        OORE_MODIFY_PATH=true
        shift
        ;;
      --no-modify-path)
        OORE_MODIFY_PATH=false
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "Unknown option: $1"
        ;;
    esac
  done
}

normalize_bool() {
  local value=""
  value="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
  case "$value" in
    true|1|yes|y|on) return 0 ;;
    false|0|no|n|off) return 1 ;;
    *) return 2 ;;
  esac
}

validate_config() {
  local legacy_status=0
  local unsigned_status=0
  [[ -n "${HOME:-}" && "$HOME" == /* ]] \
    || die 'HOME must be set to an absolute path.'

  case "$OORE_CHANNEL" in
    stable|beta|alpha) ;;
    *) die 'OORE_CHANNEL must be stable, beta, or alpha.' ;;
  esac

  [[ -n "$OORE_VERSION" ]] || die 'OORE_VERSION cannot be empty.'
  [[ "$OORE_VERSION" =~ ^[A-Za-z0-9._-]+$ ]] \
    || die 'OORE_VERSION contains unsupported characters.'
  [[ "$OORE_GITHUB_REPO" =~ ^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$ ]] \
    || die 'OORE_GITHUB_REPO must use owner/name format.'
  [[ "$OORE_INSTALL_ROOT" == /* ]] || die 'OORE_INSTALL_ROOT must be an absolute path.'
  [[ "$OORE_INSTALL_ROOT" != "/" ]] || die 'OORE_INSTALL_ROOT cannot be the filesystem root.'
  [[ "$OORE_INSTALL_ROOT" != "$HOME" ]] \
    || die 'OORE_INSTALL_ROOT cannot be the home directory.'
  [[ "$OORE_INSTALL_ROOT" != *:* ]] || die 'OORE_INSTALL_ROOT cannot contain a colon.'
  [[ "$OORE_INSTALL_ROOT" != *$'\n'* && "$OORE_INSTALL_ROOT" != *$'\r'* ]] \
    || die 'OORE_INSTALL_ROOT cannot contain a newline.'

  if [[ -z "$OORE_RELEASE_MANIFEST_URL" ]]; then
    OORE_RELEASE_MANIFEST_URL="$OORE_RELEASE_INDEX_BASE_URL/latest/$OORE_CHANNEL.json"
  fi

  case "$OORE_MODIFY_PATH" in
    auto) ;;
    *)
      if normalize_bool "$OORE_MODIFY_PATH"; then
        :
      else
        local status=$?
        [[ "$status" -eq 1 ]] || die 'OORE_MODIFY_PATH must be auto, true, or false.'
      fi
      ;;
  esac

  if [[ -n "$OORE_LEGACY_UPGRADE" ]]; then
    if normalize_bool "$OORE_LEGACY_UPGRADE"; then
      :
    else
      legacy_status=$?
      [[ "$legacy_status" -eq 1 ]] \
        || die 'OORE_LEGACY_UPGRADE must be true or false.'
    fi
  fi

  if [[ -n "$OORE_ALLOW_UNSIGNED_LOCAL_RELEASE" ]]; then
    if normalize_bool "$OORE_ALLOW_UNSIGNED_LOCAL_RELEASE"; then
      [[ "$OORE_VERSION" != latest ]] \
        || die 'OORE_ALLOW_UNSIGNED_LOCAL_RELEASE requires an exact release version.'
      is_loopback_release_base_url \
        || die 'OORE_ALLOW_UNSIGNED_LOCAL_RELEASE requires a literal loopback HTTP release origin.'
    else
      unsigned_status=$?
      [[ "$unsigned_status" -eq 1 ]] \
        || die 'OORE_ALLOW_UNSIGNED_LOCAL_RELEASE must be true or false.'
    fi
  fi
}

require_dependencies() {
  local dependency=""
  for dependency in curl tar awk sed grep head mktemp mkdir install mv rm rmdir tr dirname chmod cp sleep ssh-keygen; do
    have_command "$dependency" || die "$dependency is required."
  done

  for dependency in "$BSD_STAT" "$BSD_LOCKF"; do
    [[ -x "$dependency" ]] || die "$dependency is required."
  done

  if [[ ! -x "$BSD_SHASUM" ]] && ! have_command sha256sum; then
    die "$BSD_SHASUM or sha256sum is required."
  fi
}

is_loopback_release_base_url() {
  [[ "$OORE_RELEASE_BASE_URL" =~ ^http://(127\.0\.0\.1|\[::1\])(:[0-9]+)?(/.*)?$ ]]
}

allow_unsigned_local_release() {
  [[ -n "$OORE_ALLOW_UNSIGNED_LOCAL_RELEASE" ]] \
    && normalize_bool "$OORE_ALLOW_UNSIGNED_LOCAL_RELEASE" \
    && [[ "$OORE_VERSION" != latest ]] \
    && is_loopback_release_base_url
}

download_file() {
  local destination="$1"
  local url="$2"
  local max_time="$3"

  if allow_unsigned_local_release; then
    [[ "$url" =~ ^http://(127\.0\.0\.1|\[::1\])(:[0-9]+)?(/.*)?$ ]] \
      || die 'Unsigned local release downloads must remain on a literal loopback HTTP origin.'
    curl --disable -fsS \
      --noproxy '*' \
      --proxy '' \
      --no-location \
      --proto '=http' \
      --retry 3 \
      --connect-timeout 10 \
      --max-time "$max_time" \
      --output "$destination" \
      "$url"
  else
    curl -fsSL \
      --retry 3 \
      --connect-timeout 10 \
      --max-time "$max_time" \
      --output "$destination" \
      "$url"
  fi
}

normalized_release_public_key() {
  printf '%s\n' "$OORE_RELEASE_SIGNING_PUBLIC_KEY" | awk '
    NF >= 2 && $1 == "ssh-ed25519" { print $1 " " $2; found = 1; exit }
    END { if (!found) exit 1 }
  '
}

verify_signed_file() {
  local payload="$1"
  local signature="$2"
  local namespace="$3"
  local description="$4"
  local public_key=""
  local allowed_signers="$TMP_DIR/allowed-signers"

  public_key="$(normalized_release_public_key)" \
    || die 'This installer has no configured Oore release signing key.'
  printf '%s namespaces="%s" %s\n' \
    "$OORE_RELEASE_SIGNER_IDENTITY" "$namespace" "$public_key" > "$allowed_signers"
  chmod 0600 "$allowed_signers"
  if ! ssh-keygen -Y verify \
    -f "$allowed_signers" \
    -I "$OORE_RELEASE_SIGNER_IDENTITY" \
    -n "$namespace" \
    -s "$signature" \
    < "$payload" >/dev/null 2>&1; then
    die "$description signature verification failed."
  fi
}

download_and_verify_signature() {
  local payload="$1"
  local url="$2"
  local namespace="$3"
  local description="$4"
  local signature="$payload.sig"

  download_file "$signature" "$url.sig" 60 \
    || die "Unable to download the $description signature."
  verify_signed_file "$payload" "$signature" "$namespace" "$description"
}

detect_platform() {
  case "$(uname -s)" in
    Darwin) RELEASE_OS=darwin ;;
    Linux)
      die 'Oore CLI release assets do not support Linux yet.'
      ;;
    *)
      die "Unsupported operating system: $(uname -s)"
      ;;
  esac

  case "$(uname -m)" in
    arm64|aarch64) RELEASE_ARCH=arm64 ;;
    x86_64|amd64) RELEASE_ARCH=x86_64 ;;
    *) die "Unsupported architecture: $(uname -m)" ;;
  esac
}

infer_channel() {
  case "$1" in
    *-alpha.*) printf 'alpha' ;;
    *-beta.*) printf 'beta' ;;
    *) printf 'stable' ;;
  esac
}

resolve_release() {
  local manifest_channel=""
  local manifest_schema=""
  local tag=""
  if [[ "$OORE_VERSION" == latest ]]; then
    local manifest="$TMP_DIR/latest.json"
    download_file "$manifest" "$OORE_RELEASE_MANIFEST_URL" 60 \
      || die "Unable to fetch the latest $OORE_CHANNEL release."
    download_and_verify_signature \
      "$manifest" \
      "$OORE_RELEASE_MANIFEST_URL" \
      "$OORE_RELEASE_INDEX_NAMESPACE" \
      'release index'

    manifest_schema="$(sed -n 's/.*"schema_version"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$manifest" | head -n 1)"
    [[ "$manifest_schema" == 1 ]] \
      || die 'The signed release index uses an unsupported schema.'
    tag="$(sed -n 's/.*"tag"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest" | head -n 1)"
    if [[ -z "$tag" ]]; then
      tag="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest" | head -n 1)"
    fi
    [[ -n "$tag" ]] || die 'The release manifest does not contain a tag.'
    manifest_channel="$(sed -n 's/.*"channel"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$manifest" | head -n 1)"
    [[ "$manifest_channel" == "$OORE_CHANNEL" ]] \
      || die "The signed release index does not match the $OORE_CHANNEL channel."
    [[ "$(infer_channel "$tag")" == "$OORE_CHANNEL" ]] \
      || die "The signed release tag does not match the $OORE_CHANNEL channel."
    RESOLVED_CHANNEL="$OORE_CHANNEL"
  else
    tag="$OORE_VERSION"
    [[ "$tag" == v* ]] || tag="v$tag"
    RESOLVED_CHANNEL="$(infer_channel "$tag")"
  fi

  [[ "$tag" == v* ]] || tag="v$tag"
  RELEASE_TAG="$tag"
  RELEASE_VERSION="${tag#v}"
  [[ -n "$RELEASE_VERSION" ]] || die 'The resolved release version is empty.'
  [[ "$RELEASE_TAG" =~ ^v[A-Za-z0-9._-]+$ ]] \
    || die 'The resolved release tag contains unsupported characters.'

  CLI_ARCHIVE_NAME="oore-cli_${RELEASE_VERSION}_${RELEASE_OS}_${RELEASE_ARCH}.tar.gz"
  FULL_ARCHIVE_NAME="oore_${RELEASE_VERSION}_${RELEASE_OS}_${RELEASE_ARCH}.tar.gz"
  CHECKSUM_NAME="oore_${RELEASE_VERSION}_checksums.txt"
}

checksum_for_archive() {
  awk -v file="$1" '$2 == file { print $1; exit }' "$TMP_DIR/$CHECKSUM_NAME"
}

download_release() {
  local base_url="${OORE_RELEASE_BASE_URL%/}/$RELEASE_TAG"
  download_file "$TMP_DIR/$CHECKSUM_NAME" "$base_url/$CHECKSUM_NAME" 60 \
    || die "Unable to download $CHECKSUM_NAME."
  if allow_unsigned_local_release; then
    printf 'Using the explicit unsigned loopback acceptance path.\n'
  else
    download_and_verify_signature \
      "$TMP_DIR/$CHECKSUM_NAME" \
      "$base_url/$CHECKSUM_NAME" \
      "$OORE_RELEASE_MANIFEST_NAMESPACE" \
      'release manifest'
  fi
  MANIFEST_SHA256="$(compute_sha256 "$TMP_DIR/$CHECKSUM_NAME")"
  MANIFEST_SHA256="$(printf '%s' "$MANIFEST_SHA256" | tr '[:upper:]' '[:lower:]')"
  [[ "$MANIFEST_SHA256" =~ ^[a-f0-9]{64}$ ]] \
    || die 'The release manifest SHA-256 is invalid.'

  if [[ -n "$(checksum_for_archive "$CLI_ARCHIVE_NAME")" ]]; then
    ARCHIVE_NAME="$CLI_ARCHIVE_NAME"
  elif [[ -n "$(checksum_for_archive "$FULL_ARCHIVE_NAME")" ]]; then
    ARCHIVE_NAME="$FULL_ARCHIVE_NAME"
  else
    die "No compatible CLI archive exists for $RELEASE_TAG."
  fi

  printf 'Downloading Oore CLI %s...\n' "$RELEASE_TAG"
  download_file "$TMP_DIR/$ARCHIVE_NAME" "$base_url/$ARCHIVE_NAME" 600 \
    || die "Unable to download $ARCHIVE_NAME."
}

compute_sha256() {
  if [[ -x "$BSD_SHASUM" ]]; then
    "$BSD_SHASUM" -a 256 "$1" | awk '{ print $1 }'
  else
    sha256sum "$1" | awk '{ print $1 }'
  fi
}

verify_release() {
  local expected=""
  local actual=""
  expected="$(checksum_for_archive "$ARCHIVE_NAME")"
  [[ -n "$expected" ]] || die "No checksum exists for $ARCHIVE_NAME."
  [[ "$expected" =~ ^[A-Fa-f0-9]{64}$ ]] \
    || die "The checksum for $ARCHIVE_NAME is invalid."
  actual="$(compute_sha256 "$TMP_DIR/$ARCHIVE_NAME")"
  actual="$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')"
  expected="$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')"
  [[ "$actual" == "$expected" ]] || die "Checksum verification failed for $ARCHIVE_NAME."
  ARCHIVE_SHA256="$actual"
  printf 'Verified SHA-256 checksum.\n'
}

extract_cli() {
  local extract_dir="$TMP_DIR/extracted"
  local member_list="$TMP_DIR/archive-members.txt"
  local binary_count=""
  local binary_member=""
  local version_count=""
  local version_member=""
  local candidate=""
  local version_file=""
  local archive_version=""
  mkdir -p "$extract_dir"
  tar -tzf "$TMP_DIR/$ARCHIVE_NAME" > "$member_list" \
    || die 'The release archive cannot be inspected.'
  binary_count="$(awk '$0 == "bin/oore" || $0 == "./bin/oore" { count += 1 } END { print count + 0 }' "$member_list")"
  version_count="$(awk '$0 == "VERSION" || $0 == "./VERSION" { count += 1 } END { print count + 0 }' "$member_list")"
  [[ "$binary_count" -eq 1 ]] \
    || die 'The release archive must contain exactly one bin/oore entry.'
  [[ "$version_count" -eq 1 ]] \
    || die 'The release archive must contain exactly one VERSION entry.'
  binary_member="$(awk '$0 == "bin/oore" || $0 == "./bin/oore" { print; exit }' "$member_list")"
  version_member="$(awk '$0 == "VERSION" || $0 == "./VERSION" { print; exit }' "$member_list")"
  tar -xzf "$TMP_DIR/$ARCHIVE_NAME" -C "$extract_dir" "$binary_member" "$version_member"

  candidate="$extract_dir/${binary_member#./}"
  version_file="$extract_dir/${version_member#./}"
  [[ -f "$candidate" && ! -L "$candidate" && -x "$candidate" ]] \
    || die 'The release archive contains an invalid oore executable.'
  [[ -f "$version_file" && ! -L "$version_file" ]] \
    || die 'The release archive contains an invalid VERSION file.'
  archive_version="$(<"$version_file")"
  [[ "$archive_version" == "$RELEASE_VERSION" ]] \
    || die "The release archive VERSION does not match $RELEASE_VERSION."
  printf '%s' "$candidate"
}

prepare_cli_candidate() {
  CLI_CANDIDATE="$(extract_cli)"
  "$CLI_CANDIDATE" --help >/dev/null 2>&1 \
    || die 'The downloaded oore executable did not start.'
}

write_metadata_file() {
  local destination="$1"
  local value="$2"
  local staged=""
  staged="$(mktemp "$destination.install.XXXXXX")" || return 1
  ACTIVE_STAGED_FILE="$staged"
  if ! printf '%s\n' "$value" > "$staged" || ! chmod 0644 "$staged"; then
    rm -f "$staged"
    ACTIVE_STAGED_FILE=""
    return 1
  fi
  if ! mv -f "$staged" "$destination"; then
    rm -f "$staged"
    ACTIVE_STAGED_FILE=""
    return 1
  fi
  ACTIVE_STAGED_FILE=""
}

write_metadata() {
  write_metadata_file "$OORE_INSTALL_ROOT/VERSION" "$RELEASE_VERSION"
  write_metadata_file "$OORE_INSTALL_ROOT/CHANNEL" "$RESOLVED_CHANNEL"
  write_metadata_file "$OORE_INSTALL_ROOT/GITHUB_REPO" "$OORE_GITHUB_REPO"
  write_metadata_file "$OORE_INSTALL_ROOT/BOOTSTRAP_ARCHIVE" "$ARCHIVE_NAME"
  write_metadata_file "$OORE_INSTALL_ROOT/BOOTSTRAP_SHA256" "$ARCHIVE_SHA256"
  write_metadata_file "$OORE_INSTALL_ROOT/BOOTSTRAP_MANIFEST_SHA256" "$MANIFEST_SHA256"
}

write_shell_path_metadata() {
  if [[ -n "$SHELL_PATH_FILE" ]]; then
    write_metadata_file "$OORE_INSTALL_ROOT/SHELL_PATH_FILE" "$SHELL_PATH_FILE"
  else
    rm -f "$OORE_INSTALL_ROOT/SHELL_PATH_FILE"
  fi
}

validate_file_target() {
  local path="$1"
  if [[ -L "$path" ]]; then
    die "Refusing symbolic link at $path."
  fi
  if [[ -e "$path" ]]; then
    [[ -f "$path" ]] || die "Expected a regular file at $path."
    [[ -O "$path" ]] || die "The file is not owned by the current user: $path"
  fi
}

directory_permissions_are_safe() {
  local mode=""
  mode="$("$BSD_STAT" -f '%Lp' "$1")" || return 1
  [[ "$mode" =~ ^[0-7]+$ ]] || return 1
  (( (8#$mode & 0022) == 0 ))
}

preflight_install_root() {
  local bin_dir="$OORE_INSTALL_ROOT/bin"
  local canonical_home=""
  local canonical_root=""
  local install_parent=""
  install_parent="$(dirname "$OORE_INSTALL_ROOT")"
  [[ -d "$install_parent" ]] \
    || die "The install root parent must exist: $install_parent"
  [[ -O "$install_parent" && -w "$install_parent" ]] \
    || die "The install root parent must be owned and writable by the current user: $install_parent"
  directory_permissions_are_safe "$install_parent" \
    || die "The install root parent is writable by another user: $install_parent"
  if [[ -L "$OORE_INSTALL_ROOT" ]]; then
    die "The install root cannot be a symbolic link: $OORE_INSTALL_ROOT"
  elif [[ -e "$OORE_INSTALL_ROOT" ]]; then
    [[ -d "$OORE_INSTALL_ROOT" ]] \
      || die "The install root is not a directory: $OORE_INSTALL_ROOT"
    canonical_root="$(cd "$OORE_INSTALL_ROOT" && pwd -P)" \
      || die "The install root cannot be resolved: $OORE_INSTALL_ROOT"
    [[ "$canonical_root" == "$OORE_INSTALL_ROOT" ]] \
      || die 'OORE_INSTALL_ROOT cannot contain symbolic-link or dot path segments.'
    canonical_home="$(cd "$HOME" && pwd -P)" \
      || die 'The home directory cannot be resolved.'
    [[ "$canonical_root" != "/" && "$canonical_root" != "$canonical_home" ]] \
      || die 'OORE_INSTALL_ROOT resolves to a protected broad directory.'
    [[ -O "$OORE_INSTALL_ROOT" ]] \
      || die "The install root is not owned by the current user: $OORE_INSTALL_ROOT"
    [[ -w "$OORE_INSTALL_ROOT" ]] \
      || die "The install root is not writable: $OORE_INSTALL_ROOT"
    directory_permissions_are_safe "$OORE_INSTALL_ROOT" \
      || die "The install root is writable by another user: $OORE_INSTALL_ROOT"
  else
    canonical_root="$(cd "$install_parent" && pwd -P)/${OORE_INSTALL_ROOT##*/}" \
      || die "The install root parent cannot be resolved: $install_parent"
    [[ "$canonical_root" == "$OORE_INSTALL_ROOT" ]] \
      || die 'OORE_INSTALL_ROOT cannot contain symbolic-link or dot path segments.'
    return 0
  fi

  if [[ -L "$bin_dir" ]]; then
    die "The Oore bin directory cannot be a symbolic link: $bin_dir"
  elif [[ -e "$bin_dir" ]]; then
    [[ -d "$bin_dir" ]] || die "The Oore bin path is not a directory: $bin_dir"
    [[ -O "$bin_dir" ]] || die "The Oore bin directory has an unexpected owner: $bin_dir"
    [[ -w "$bin_dir" ]] || die "The Oore bin directory is not writable: $bin_dir"
    directory_permissions_are_safe "$bin_dir" \
      || die "The Oore bin directory is writable by another user: $bin_dir"
  fi

  validate_file_target "$bin_dir/oore"
}

validate_guard_file() {
  local path="$1"
  local description="$2"
  local maximum_size="$3"
  local access="$4"
  local metadata=""
  local device=""
  local inode=""
  local owner=""
  local mode=""
  local links=""
  local size=""
  local modified=""
  local changed=""

  [[ ! -L "$path" ]] || die "$description cannot be a symbolic link: $path"
  [[ -f "$path" ]] || die "$description is not a regular file: $path"
  [[ -O "$path" ]] || die "$description is not owned by the current user: $path"
  metadata="$("$BSD_STAT" -f "$FILE_IDENTITY_FORMAT" "$path")" \
    || die "Could not inspect $description: $path"
  IFS=: read -r device inode owner mode links size modified changed <<< "$metadata"
  [[ "$device" =~ ^[0-9]+$ && "$inode" =~ ^[0-9]+$ && "$owner" =~ ^[0-9]+$ \
    && "$mode" =~ ^[0-7]+$ && "$links" =~ ^[0-9]+$ && "$size" =~ ^[0-9]+$ \
    && "$modified" =~ ^[0-9]+$ && "$changed" =~ ^[0-9]+$ ]] \
    || die "$description has ambiguous metadata: $path"
  [[ "$links" -eq 1 ]] || die "$description cannot be hard-linked: $path"
  [[ "$size" -gt 0 && "$size" -le "$maximum_size" ]] \
    || die "$description has an unsafe size: $path"
  if [[ "$access" == private ]]; then
    (( (8#$mode & 0077) == 0 )) \
      || die "$description must not grant group or other access: $path"
  else
    (( (8#$mode & 0022) == 0 )) \
      || die "$description is writable by another user: $path"
  fi
  GUARD_FILE_IDENTITY="$metadata"
}

inspect_bootstrap_boundary() {
  local manifest="$OORE_INSTALL_ROOT/install-manifest.json"
  local cli="$OORE_INSTALL_ROOT/bin/oore"
  local manifest_identity=""
  local cli_identity=""
  local guard_cli="$CLI_CANDIDATE"
  local action=""
  local preserved_profile=""

  if [[ -e "$manifest" || -L "$manifest" ]]; then
    validate_guard_file "$manifest" 'The installation profile manifest' 1048576 private
    manifest_identity="$GUARD_FILE_IDENTITY"
    validate_guard_file "$cli" 'The installed Oore CLI' 1073741824 metadata
    cli_identity="$GUARD_FILE_IDENTITY"
    [[ -x "$cli" ]] || die "The installed Oore CLI is not executable: $cli"
    guard_cli="$cli"
  fi

  action="$(OORE_INSTALL_ROOT="$OORE_INSTALL_ROOT" "$guard_cli" bootstrap-guard \
    --target-version "$RELEASE_VERSION" \
    --target-channel "$RESOLVED_CHANNEL" \
    --target-repository "$OORE_GITHUB_REPO")" \
    || die 'The Oore CLI could not approve this bootstrap.'
  case "$action" in
    update|legacy-v0.1.41) ;;
    profile-preserve=*)
      preserved_profile="${action#profile-preserve=}"
      case "$preserved_profile" in
        complete|control-plane|runner|web-node) ;;
        *) die 'The Oore CLI returned an invalid preserved profile.' ;;
      esac
      action=profile-preserve
      ;;
    *) die 'The Oore CLI returned an invalid bootstrap action.' ;;
  esac

  if [[ -n "$manifest_identity" ]]; then
    [[ "$("$BSD_STAT" -f "$FILE_IDENTITY_FORMAT" "$manifest")" == "$manifest_identity" ]] \
      || die "The installation profile manifest changed while it was inspected: $manifest"
    [[ "$("$BSD_STAT" -f "$FILE_IDENTITY_FORMAT" "$cli")" == "$cli_identity" ]] \
      || die "The installed Oore CLI changed while it was inspected: $cli"
  fi
  BOOTSTRAP_ACTION="$action"
  PRESERVED_PROFILE="$preserved_profile"
}

remove_legacy_install() {
  if can_prompt; then
    OORE_INSTALL_ROOT="$OORE_INSTALL_ROOT" \
      "$CLI_CANDIDATE" uninstall --legacy-v0-1-41 < /dev/tty
  elif [[ -n "$OORE_LEGACY_UPGRADE" ]] && normalize_bool "$OORE_LEGACY_UPGRADE"; then
    OORE_INSTALL_ROOT="$OORE_INSTALL_ROOT" \
      "$CLI_CANDIDATE" uninstall --legacy-v0-1-41 --yes
  else
    printf 'Oore install failed: A verified v0.1.41 removal plan requires explicit approval.\n' >&2
    printf 'Rerun the same bootstrap with OORE_LEGACY_UPGRADE=true.\n' >&2
    exit 1
  fi

  preflight_install_root
  inspect_bootstrap_boundary
  [[ "$BOOTSTRAP_ACTION" == update ]] \
    || die 'Legacy removal did not complete. No bootstrap changes were made.'
}

acquire_lifecycle_lock() {
  local lock_parent=""
  local root_name="${OORE_INSTALL_ROOT##*/}"
  lock_parent="$(dirname "$OORE_INSTALL_ROOT")"
  LIFECYCLE_LOCK_PATH="$lock_parent/.$root_name.oore-lifecycle.lock"
  validate_file_target "$LIFECYCLE_LOCK_PATH"
  if [[ ! -e "$LIFECYCLE_LOCK_PATH" ]]; then
    (umask 077; : > "$LIFECYCLE_LOCK_PATH") \
      || die "Could not create the installation lock: $LIFECYCLE_LOCK_PATH"
  fi
  validate_file_target "$LIFECYCLE_LOCK_PATH"
  chmod 0600 "$LIFECYCLE_LOCK_PATH"
  exec 9<> "$LIFECYCLE_LOCK_PATH"
  local status=0
  "$BSD_LOCKF" -s -t 0 9 2>/dev/null || status=$?
  case "$status" in
    0) ;;
    # EX_USAGE: lockf before macOS 15 cannot lock a descriptor.
    64)
      exec 9>&-
      hold_lifecycle_lock_by_path
      ;;
    75) die 'Another Oore install, setup, update, or uninstall operation is active.' ;;
    *) die "Could not lock the installation: $LIFECYCLE_LOCK_PATH" ;;
  esac
}

# Older lockf can only lock a path for the lifetime of a command, so a helper
# holds the lock until this script exits. Both forms take a flock(2) lock, the
# same lock the Oore CLI takes.
hold_lifecycle_lock_by_path() {
  local ready="$TMP_DIR/lifecycle-lock.ready"
  # shellcheck disable=SC2016 # The helper shell expands its own arguments.
  "$BSD_LOCKF" -k -s -t 0 "$LIFECYCLE_LOCK_PATH" /bin/sh -c \
    ': > "$1"; while kill -0 "$2" 2>/dev/null; do sleep 1; done' \
    oore-lifecycle-lock "$ready" "$$" < /dev/null > /dev/null 2>&1 &
  LIFECYCLE_LOCK_HOLDER_PID=$!
  while [[ ! -e "$ready" ]] && kill -0 "$LIFECYCLE_LOCK_HOLDER_PID" 2>/dev/null; do
    sleep 0.1
  done
  [[ -e "$ready" ]] \
    || die 'Another Oore install, setup, update, or uninstall operation is active.'
}

snapshot_directory_state() {
  local bin_dir="$OORE_INSTALL_ROOT/bin"
  if [[ -d "$OORE_INSTALL_ROOT" && ! -L "$OORE_INSTALL_ROOT" ]]; then
    INSTALL_ROOT_EXISTED=1
    INSTALL_ROOT_ORIGINAL_MODE="$("$BSD_STAT" -f '%Lp' "$OORE_INSTALL_ROOT")"
  fi
  if [[ -d "$bin_dir" && ! -L "$bin_dir" ]]; then
    BIN_DIR_EXISTED=1
    BIN_DIR_ORIGINAL_MODE="$("$BSD_STAT" -f '%Lp' "$bin_dir")"
  fi
}

prepare_install_root() {
  local bin_dir="$OORE_INSTALL_ROOT/bin"
  if [[ "$INSTALL_ROOT_EXISTED" -eq 0 ]]; then
    install -d -m 0700 "$OORE_INSTALL_ROOT"
  fi

  if [[ "$BIN_DIR_EXISTED" -eq 0 ]]; then
    install -d -m 0755 "$bin_dir"
  fi
}

snapshot_install_state() {
  local relative=""
  local target=""
  local backup=""
  SNAPSHOT_DIR="$TMP_DIR/prior-install"
  mkdir -p "$SNAPSHOT_DIR"

  for relative in "${BOOTSTRAP_PATHS[@]}"; do
    if [[ "$relative" == "bin/oore" && "$CLI_PUBLICATION_SKIPPED" -eq 1 ]]; then
      continue
    fi
    target="$OORE_INSTALL_ROOT/$relative"
    validate_file_target "$target"
    if [[ -f "$target" ]]; then
      backup="$SNAPSHOT_DIR/$relative"
      mkdir -p "$(dirname "$backup")"
      cp -p "$target" "$backup"
    fi
  done

  if [[ "$PATH_ACTION" == "append" ]]; then
    PATH_RC_SNAPSHOT="$SNAPSHOT_DIR/shell-rc"
    if [[ -f "$SHELL_RC" ]]; then
      cp -p "$SHELL_RC" "$PATH_RC_SNAPSHOT"
      PATH_RC_EXISTED=1
    fi
  fi
}

rollback_install() {
  local relative=""
  local target=""
  local backup=""
  local staged=""
  local failed=0

  if [[ -n "$ACTIVE_STAGED_FILE" ]] && ! rm -f "$ACTIVE_STAGED_FILE"; then
    failed=1
  fi
  ACTIVE_STAGED_FILE=""

  for relative in "${BOOTSTRAP_PATHS[@]}"; do
    if [[ "$relative" == "bin/oore" && "$CLI_PUBLICATION_SKIPPED" -eq 1 ]]; then
      continue
    fi
    target="$OORE_INSTALL_ROOT/$relative"
    backup="$SNAPSHOT_DIR/$relative"
    if [[ -f "$backup" ]]; then
      staged="$target.rollback.$$"
      rm -f "$staged" || failed=1
      if cp -p "$backup" "$staged" && mv -f "$staged" "$target"; then
        :
      else
        rm -f "$staged"
        failed=1
      fi
    elif ! rm -f "$target"; then
      failed=1
    fi
  done

  if [[ "$PATH_RC_MUTATION_STARTED" -eq 1 ]]; then
    if [[ "$PATH_RC_EXISTED" -eq 1 ]]; then
      cp -p "$PATH_RC_SNAPSHOT" "$SHELL_RC" || failed=1
    else
      rm -f "$SHELL_RC" || failed=1
    fi
  fi

  if [[ "$BIN_DIR_EXISTED" -eq 1 ]]; then
    chmod "$BIN_DIR_ORIGINAL_MODE" "$OORE_INSTALL_ROOT/bin" || failed=1
  elif [[ -d "$OORE_INSTALL_ROOT/bin" && ! -L "$OORE_INSTALL_ROOT/bin" ]]; then
    rmdir "$OORE_INSTALL_ROOT/bin" >/dev/null 2>&1 || failed=1
  fi

  if [[ "$INSTALL_ROOT_EXISTED" -eq 1 ]]; then
    chmod "$INSTALL_ROOT_ORIGINAL_MODE" "$OORE_INSTALL_ROOT" || failed=1
  elif [[ -d "$OORE_INSTALL_ROOT" && ! -L "$OORE_INSTALL_ROOT" ]]; then
    rmdir "$OORE_INSTALL_ROOT" >/dev/null 2>&1 || failed=1
  fi
  return "$failed"
}

install_cli() {
  local destination="$OORE_INSTALL_ROOT/bin/oore"
  local candidate_sha256=""
  local installed_sha256=""
  local staged=""

  preflight_install_root
  if [[ -x "$destination" ]] && directory_permissions_are_safe "$destination"; then
    candidate_sha256="$(compute_sha256 "$CLI_CANDIDATE")"
    installed_sha256="$(compute_sha256 "$destination")"
    if [[ "$candidate_sha256" == "$installed_sha256" ]]; then
      CLI_PUBLICATION_SKIPPED=1
    fi
  fi
  snapshot_directory_state
  snapshot_install_state
  INSTALL_TRANSACTION_ACTIVE=1
  prepare_install_root
  if [[ "$CLI_PUBLICATION_SKIPPED" -eq 0 ]]; then
    staged="$(mktemp "$destination.install.XXXXXX")"
    ACTIVE_STAGED_FILE="$staged"
    if ! install -m 0755 "$CLI_CANDIDATE" "$staged"; then
      rm -f "$staged"
      ACTIVE_STAGED_FILE=""
      die 'Could not stage the oore executable.'
    fi
    if ! mv -f "$staged" "$destination"; then
      rm -f "$staged"
      ACTIVE_STAGED_FILE=""
      die 'Could not publish the oore executable.'
    fi
    ACTIVE_STAGED_FILE=""
  fi
  write_metadata
}

verify_published_install() {
  local destination="$OORE_INSTALL_ROOT/bin/oore"
  [[ -f "$destination" && ! -L "$destination" && -x "$destination" ]] \
    || die 'The installed oore executable failed verification.'
  "$destination" --help >/dev/null 2>&1 \
    || die 'The installed oore executable did not start.'
  if "$destination" install --help >/dev/null 2>&1; then
    GUIDED_INSTALL_SUPPORTED=1
  else
    GUIDED_INSTALL_SUPPORTED=0
  fi
  [[ "$(<"$OORE_INSTALL_ROOT/VERSION")" == "$RELEASE_VERSION" ]] \
    || die 'The installed VERSION metadata failed verification.'
  [[ "$(<"$OORE_INSTALL_ROOT/CHANNEL")" == "$RESOLVED_CHANNEL" ]] \
    || die 'The installed CHANNEL metadata failed verification.'
  [[ "$(<"$OORE_INSTALL_ROOT/GITHUB_REPO")" == "$OORE_GITHUB_REPO" ]] \
    || die 'The installed repository metadata failed verification.'
  [[ "$(<"$OORE_INSTALL_ROOT/BOOTSTRAP_ARCHIVE")" == "$ARCHIVE_NAME" ]] \
    || die 'The installed archive metadata failed verification.'
  [[ "$(<"$OORE_INSTALL_ROOT/BOOTSTRAP_SHA256")" == "$ARCHIVE_SHA256" ]] \
    || die 'The installed SHA-256 metadata failed verification.'
  [[ "$(<"$OORE_INSTALL_ROOT/BOOTSTRAP_MANIFEST_SHA256")" == "$MANIFEST_SHA256" ]] \
    || die 'The installed release manifest metadata failed verification.'
  if [[ -n "$SHELL_PATH_FILE" ]]; then
    [[ -f "$OORE_INSTALL_ROOT/SHELL_PATH_FILE" \
      && ! -L "$OORE_INSTALL_ROOT/SHELL_PATH_FILE" \
      && "$(<"$OORE_INSTALL_ROOT/SHELL_PATH_FILE")" == "$SHELL_PATH_FILE" ]] \
      || die 'The installed shell PATH metadata failed verification.'
  else
    [[ ! -e "$OORE_INSTALL_ROOT/SHELL_PATH_FILE" \
      && ! -L "$OORE_INSTALL_ROOT/SHELL_PATH_FILE" ]] \
      || die 'Unexpected shell PATH metadata remains installed.'
  fi
}

can_prompt() {
  [[ -r /dev/tty && -w /dev/tty ]] || return 1
  (: > /dev/tty) 2>/dev/null
}

detect_shell_rc() {
  case "${SHELL:-}" in
    */zsh) SHELL_RC="$HOME/.zshrc" ;;
    */bash)
      if [[ "${RELEASE_OS:-}" == darwin ]]; then
        SHELL_RC="$HOME/.bash_profile"
      else
        SHELL_RC="$HOME/.bashrc"
      fi
      ;;
    *) SHELL_RC="" ;;
  esac
}

shell_quote() {
  printf '%q' "$1"
}

escape_double_quoted() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//\$/\\\$}"
  value="${value//\`/\\\`}"
  printf '%s' "$value"
}

validate_managed_path_block() {
  OORE_PATH_EXPORT_LINE="$PATH_EXPORT_LINE" awk \
    -v start="$PATH_BLOCK_START" \
    -v end="$PATH_BLOCK_END" '
      BEGIN { export_line = ENVIRON["OORE_PATH_EXPORT_LINE"] }
      {
        if (expected == 1) {
          if ($0 != export_line) bad = 1
          expected = 2
          next
        }
        if (expected == 2) {
          if ($0 != end) bad = 1
          else ends += 1
          expected = 0
          next
        }
        if ($0 == start) {
          starts += 1
          expected = 1
          next
        }
        if ($0 == end) {
          ends += 1
          bad = 1
        }
      }
      END {
        if (starts == 0 && ends == 0) exit 3
        if (bad || expected != 0 || starts != 1 || ends != 1) exit 1
        exit 0
      }
    ' "$SHELL_RC"
}

should_modify_path() {
  case "$OORE_MODIFY_PATH" in
    auto)
      can_prompt || return 1
      printf '\nAdd %s/bin to your shell PATH? [Y/n] ' "$OORE_INSTALL_ROOT" > /dev/tty
      local answer=""
      read -r answer < /dev/tty || answer=""
      [[ -z "$answer" || "$answer" =~ ^[Yy]([Ee][Ss])?$ ]]
      ;;
    *) normalize_bool "$OORE_MODIFY_PATH" ;;
  esac
}

preflight_path() {
  local escaped_bin=""
  local block_status=0
  local path_required=0
  local shell_parent=""

  detect_shell_rc
  if [[ "$OORE_MODIFY_PATH" != auto ]] && normalize_bool "$OORE_MODIFY_PATH"; then
    path_required=1
  fi
  if [[ -z "$SHELL_RC" ]]; then
    [[ "$path_required" -eq 0 ]] \
      || die '--modify-path requires SHELL to identify zsh or bash.'
    return 0
  fi

  if [[ -L "$SHELL_RC" ]]; then
    if should_modify_path; then
      die "The shell configuration cannot be a symbolic link: $SHELL_RC"
    fi
    return 0
  fi

  escaped_bin="$(escape_double_quoted "$OORE_INSTALL_ROOT/bin")"
  PATH_EXPORT_LINE="export PATH=\"$escaped_bin:\$PATH\""

  if [[ -f "$SHELL_RC" ]]; then
    [[ -O "$SHELL_RC" ]] || die "The shell configuration has an unexpected owner: $SHELL_RC"
    [[ -r "$SHELL_RC" ]] || die "The shell configuration is not readable: $SHELL_RC"
    if validate_managed_path_block; then
      directory_permissions_are_safe "$SHELL_RC" \
        || die "The shell configuration is writable by another user: $SHELL_RC"
      SHELL_PATH_FILE="${SHELL_RC##*/}"
      return 0
    else
      block_status=$?
      [[ "$block_status" -eq 3 ]] \
        || die "The managed Oore PATH block in $SHELL_RC is not exactly three adjacent lines."
    fi

    if grep -Fqx "$PATH_EXPORT_LINE" "$SHELL_RC"; then
      return 0
    fi
  elif [[ -e "$SHELL_RC" || -L "$SHELL_RC" ]]; then
    if should_modify_path; then
      die "The shell configuration is not a regular file: $SHELL_RC"
    fi
    return 0
  fi

  should_modify_path || return 0
  shell_parent="$(dirname "$SHELL_RC")"
  [[ -d "$shell_parent" && -O "$shell_parent" && -w "$shell_parent" ]] \
    || die "The shell configuration directory must be owned and writable by the current user: $shell_parent"
  directory_permissions_are_safe "$shell_parent" \
    || die "The shell configuration directory is writable by another user: $shell_parent"
  if [[ -f "$SHELL_RC" ]]; then
    [[ -w "$SHELL_RC" ]] || die "The shell configuration is not writable: $SHELL_RC"
    directory_permissions_are_safe "$SHELL_RC" \
      || die "The shell configuration is writable by another user: $SHELL_RC"
  fi
  PATH_ACTION="append"
  SHELL_PATH_FILE="${SHELL_RC##*/}"
}

configure_path() {
  local staged=""
  [[ "$PATH_ACTION" == "append" ]] || return 0
  mkdir -p "$(dirname "$SHELL_RC")"
  staged="$(mktemp "$SHELL_RC.oore-path.XXXXXX")"
  ACTIVE_STAGED_FILE="$staged"
  if [[ -f "$SHELL_RC" ]]; then
    cp -p "$SHELL_RC" "$staged"
  else
    chmod 0600 "$staged"
  fi
  printf '\n%s\n%s\n%s\n' \
    "$PATH_BLOCK_START" "$PATH_EXPORT_LINE" "$PATH_BLOCK_END" >> "$staged"
  PATH_RC_MUTATION_STARTED=1
  mv -f "$staged" "$SHELL_RC"
  ACTIVE_STAGED_FILE=""
  PATH_UPDATED=1
  printf 'Updated %s\n' "$SHELL_RC"
}

print_next_step() {
  printf '\nOore CLI %s is ready.\n' "$RELEASE_VERSION"
  if [[ "$GUIDED_INSTALL_SUPPORTED" -eq 0 ]]; then
    printf 'This release predates guided device setup.\n'
    printf 'Oore v0.1.42 or newer is required for that flow.\n'
    return 0
  fi
  if [[ ":$PATH:" != *":$OORE_INSTALL_ROOT/bin:"* ]]; then
    if [[ "$PATH_UPDATED" -eq 1 && -n "$SHELL_RC" ]]; then
      printf 'Next: source %s && oore install\n' "$(shell_quote "$SHELL_RC")"
    else
      printf 'Next: %s install\n' "$(shell_quote "$OORE_INSTALL_ROOT/bin/oore")"
    fi
  else
    printf 'Next: oore install\n'
  fi
}

main() {
  parse_args "$@"
  validate_config
  detect_platform
  require_dependencies
  preflight_install_root
  TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/oore-install.XXXXXX")"
  resolve_release
  download_release
  verify_release
  prepare_cli_candidate
  inspect_bootstrap_boundary
  if [[ "$BOOTSTRAP_ACTION" == legacy-v0.1.41 ]]; then
    remove_legacy_install
  fi

  preflight_install_root
  acquire_lifecycle_lock
  preflight_install_root
  inspect_bootstrap_boundary
  case "$BOOTSTRAP_ACTION" in
    profile-preserve)
      printf '\nThe existing profile already records release %s.\n' "$RELEASE_VERSION"
      printf 'No CLI or release metadata was changed.\n'
      printf 'Next: %s install --profile %s\n' \
        "$(shell_quote "$OORE_INSTALL_ROOT/bin/oore")" "$PRESERVED_PROFILE"
      return 0
      ;;
    legacy-v0.1.41)
      die 'A legacy Oore installation appeared while the bootstrap lock was acquired.'
      ;;
    update) ;;
  esac

  preflight_path
  install_cli
  configure_path
  write_shell_path_metadata
  verify_published_install
  INSTALL_TRANSACTION_ACTIVE=0
  printf 'Installed %s\n' "$OORE_INSTALL_ROOT/bin/oore"
  print_next_step
}

main "$@"
