#!/usr/bin/env bash
set -euo pipefail

if [[ "$#" -ne 10 ]]; then
  echo "Usage: $0 <tag> <output> <full-arm64> <full-x86_64> <cli-arm64> <cli-x86_64> <web-darwin-arm64> <web-darwin-x86_64> <web-linux-arm64> <web-linux-x86_64>" >&2
  exit 2
fi

tag="$1"
output_dir="$2"
full_arm64="$3"
full_x86_64="$4"
cli_arm64="$5"
cli_x86_64="$6"
web_darwin_arm64="$7"
web_darwin_x86_64="$8"
web_linux_arm64="$9"
web_linux_x86_64="${10}"

bash "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/validate-release-tag.sh" "$tag"
version="${tag#v}"

require_file() {
  local path="$1"
  [[ -f "$path" && ! -L "$path" ]] || {
    echo "Release stage lacks a regular file: $path" >&2
    exit 1
  }
}

require_executable() {
  local path="$1"
  require_file "$path"
  [[ -x "$path" ]] || {
    echo "Release stage file is not executable: $path" >&2
    exit 1
  }
}

require_stage_directory() {
  local stage="$1"
  [[ -d "$stage" && ! -L "$stage" ]] || {
    echo "Release stage is not a regular directory: $stage" >&2
    exit 1
  }
}

require_version() {
  local stage="$1"
  require_file "$stage/VERSION"
  [[ "$(<"$stage/VERSION")" == "$version" ]] || {
    echo "Release stage version does not match $version: $stage/VERSION" >&2
    exit 1
  }
}

validate_stage_tree() {
  local stage="$1"
  local kind="$2"
  local path=""
  local relative=""
  local links=""

  while IFS= read -r -d '' path; do
    relative="${path#"$stage"/}"
    if [[ -L "$path" ]]; then
      echo "Release stage contains a symbolic link: $path" >&2
      exit 1
    fi
    if [[ -f "$path" ]]; then
      links="$(stat -f '%l' "$path")"
      [[ "$links" -eq 1 ]] || {
        echo "Release stage contains a hard-linked file: $path" >&2
        exit 1
      }
    elif [[ ! -d "$path" ]]; then
      echo "Release stage contains a special entry: $path" >&2
      exit 1
    fi

    case "$kind:$relative" in
      full:LICENSE|full:VERSION|full:bin|full:bin/oore|full:bin/oored|full:bin/oore-web|full:web-dist|full:web-dist/*) ;;
      web:LICENSE|web:VERSION|web:bin|web:bin/oore-web|web:web-dist|web:web-dist/*) ;;
      *)
        echo "Release stage contains an unexpected path: $path" >&2
        exit 1
        ;;
    esac
  done < <(find "$stage" -mindepth 1 -print0)
}

validate_full_stage() {
  local stage="$1"
  local obsolete
  require_stage_directory "$stage"
  require_executable "$stage/bin/oore"
  require_executable "$stage/bin/oored"
  require_executable "$stage/bin/oore-web"
  require_file "$stage/LICENSE"
  require_version "$stage"
  [[ -d "$stage/web-dist" ]] || {
    echo "Release stage lacks web-dist: $stage" >&2
    exit 1
  }
  require_file "$stage/web-dist/index.html"
  validate_stage_tree "$stage" full
}

validate_cli_stage() {
  local stage="$1"
  local actual expected
  require_stage_directory "$stage"
  require_executable "$stage/bin/oore"
  require_file "$stage/LICENSE"
  require_version "$stage"

  actual="$(cd "$stage" && find . -mindepth 1 -print | LC_ALL=C sort)"
  expected="$(printf '%s\n' ./LICENSE ./VERSION ./bin ./bin/oore | LC_ALL=C sort)"
  [[ "$actual" == "$expected" ]] || {
    echo "CLI release stage contains unexpected paths: $stage" >&2
    diff -u <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") >&2 || true
    exit 1
  }
}

validate_web_stage() {
  local stage="$1"
  require_stage_directory "$stage"
  require_executable "$stage/bin/oore-web"
  require_file "$stage/LICENSE"
  require_version "$stage"
  [[ -d "$stage/web-dist" ]] || {
    echo "Web release stage lacks web-dist: $stage" >&2
    exit 1
  }
  require_file "$stage/web-dist/index.html"
  validate_stage_tree "$stage" web
}

validate_full_stage "$full_arm64"
validate_full_stage "$full_x86_64"
validate_cli_stage "$cli_arm64"
validate_cli_stage "$cli_x86_64"
validate_web_stage "$web_darwin_arm64"
validate_web_stage "$web_darwin_x86_64"
validate_web_stage "$web_linux_arm64"
validate_web_stage "$web_linux_x86_64"

cmp -s "$full_arm64/bin/oore" "$cli_arm64/bin/oore" || {
  echo 'Full and CLI-only arm64 bundles contain different oore binaries.' >&2
  exit 1
}
cmp -s "$full_x86_64/bin/oore" "$cli_x86_64/bin/oore" || {
  echo 'Full and CLI-only x86_64 bundles contain different oore binaries.' >&2
  exit 1
}

assets=(
  "oore_${version}_darwin_arm64.tar.gz"
  "oore_${version}_darwin_x86_64.tar.gz"
  "oore-cli_${version}_darwin_arm64.tar.gz"
  "oore-cli_${version}_darwin_x86_64.tar.gz"
  "oore-web_${version}_darwin_arm64.tar.gz"
  "oore-web_${version}_darwin_x86_64.tar.gz"
  "oore-web_${version}_linux_arm64.tar.gz"
  "oore-web_${version}_linux_x86_64.tar.gz"
)
stages=(
  "$full_arm64"
  "$full_x86_64"
  "$cli_arm64"
  "$cli_x86_64"
  "$web_darwin_arm64"
  "$web_darwin_x86_64"
  "$web_linux_arm64"
  "$web_linux_x86_64"
)

mkdir -p "$output_dir"
for index in "${!assets[@]}"; do
  COPYFILE_DISABLE=1 tar --no-xattrs -C "${stages[$index]}" -czf "$output_dir/${assets[$index]}" .
done

checksums="oore_${version}_checksums.txt"
(
  cd "$output_dir"
  shasum -a 256 "${assets[@]}" > "$checksums"
  shasum -a 256 -c "$checksums" >/dev/null
)
