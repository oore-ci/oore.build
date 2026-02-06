#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

FEATURE_DIR="docs/features"

if [[ ! -d "$FEATURE_DIR" ]]; then
  echo "missing docs/features directory" >&2
  exit 1
fi

required_headers=(
  "## Status"
  "## Problem"
  "## User Impact"
  "## UI Changes"
  "## API Changes"
  "## Security Considerations"
  "## Migration and Rollout"
  "## Acceptance Criteria"
  "## Owner"
  "## Last Updated"
)

check_doc_file() {
  local file="$1"
  for header in "${required_headers[@]}"; do
    if ! grep -qF "$header" "$file"; then
      echo "missing required section '$header' in $file" >&2
      return 1
    fi
  done
}

feature_docs=()
while IFS= read -r file; do
  feature_docs+=("$file")
done < <(find "$FEATURE_DIR" -maxdepth 1 -type f -name "*.md" ! -name "README.md" | sort)

if [[ "${#feature_docs[@]}" -eq 0 ]]; then
  echo "no feature docs found in $FEATURE_DIR" >&2
fi

for file in "${feature_docs[@]}"; do
  check_doc_file "$file"
done

BASE_SHA="${BASE_SHA:-}"
HEAD_SHA="${HEAD_SHA:-HEAD}"

if [[ -z "$BASE_SHA" ]]; then
  if git rev-parse HEAD~1 >/dev/null 2>&1; then
    BASE_SHA="HEAD~1"
  fi
fi

if [[ -n "$BASE_SHA" ]]; then
  changed_files="$(git diff --name-only "$BASE_SHA" "$HEAD_SHA")"

  # Include uncommitted and untracked changes so the gate works pre-commit
  if [[ "$HEAD_SHA" == "HEAD" ]]; then
    uncommitted="$(git diff --name-only HEAD 2>/dev/null || true)"
    untracked="$(git ls-files --others --exclude-standard 2>/dev/null || true)"
    changed_files="$(printf '%s\n%s\n%s' "$changed_files" "$uncommitted" "$untracked" | sort -u)"
  fi

  if [[ -n "$changed_files" ]]; then
    code_changed="false"
    docs_changed="false"

    while IFS= read -r file; do
      [[ -z "$file" ]] && continue

      if [[ "$file" =~ ^(apps/|crates/|backend/|frontend/|src/|oore/|oored/|runner/|scripts/) ]]; then
        code_changed="true"
      fi

      if [[ "$file" =~ ^docs/features/.*\.md$ && "$file" != "docs/features/README.md" ]]; then
        docs_changed="true"
      fi
    done <<< "$changed_files"

    if [[ "$code_changed" == "true" && "$docs_changed" == "false" ]]; then
      echo "code changes detected without docs/features update" >&2
      exit 1
    fi
  fi
fi

echo "feature docs check passed"
