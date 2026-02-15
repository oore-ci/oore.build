#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

DOCS_INDEX="docs/README.md"
DOCS_LEDGER="docs/changes.md"

if [[ ! -f "$DOCS_INDEX" ]]; then
  echo "missing $DOCS_INDEX (internal docs index pointer)" >&2
  exit 1
fi

if [[ ! -f "$DOCS_LEDGER" ]]; then
  echo "missing $DOCS_LEDGER (required change ledger)" >&2
  exit 1
fi

if ! grep -qE '^## ' "$DOCS_LEDGER"; then
  echo "$DOCS_LEDGER has no entries (expected at least one '## YYYY-MM-DD' section)" >&2
  exit 1
fi

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
    ledger_changed="false"

    while IFS= read -r file; do
      [[ -z "$file" ]] && continue

      if [[ "$file" =~ ^(apps/|crates/|backend/|frontend/|src/|oore/|oored/|runner/|scripts/) ]]; then
        code_changed="true"
      fi

      if [[ "$file" == "$DOCS_LEDGER" ]]; then
        ledger_changed="true"
      fi
    done <<< "$changed_files"

    if [[ "$code_changed" == "true" && "$ledger_changed" == "false" ]]; then
      echo "code changes detected without updating $DOCS_LEDGER" >&2
      exit 1
    fi
  fi
fi

echo "docs gate passed"
