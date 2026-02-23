#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WRANGLER_BIN="${WRANGLER:-./node_modules/.bin/wrangler}"
PAGES_BRANCH="${PAGES_BRANCH:-}"
PAGES_COMMIT_HASH="${PAGES_COMMIT_HASH:-}"
PAGES_VERIFY_MODE="${PAGES_VERIFY_MODE:-parallel}"
PAGES_VERIFY_ATTEMPTS="${PAGES_VERIFY_ATTEMPTS:-24}"
PAGES_VERIFY_SLEEP_SECONDS="${PAGES_VERIFY_SLEEP_SECONDS:-5}"

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required for Pages deployment verification." >&2
  exit 1
fi

if [[ "$WRANGLER_BIN" == */* ]]; then
  if [[ ! -x "$WRANGLER_BIN" ]]; then
    echo "Wrangler binary not found/executable: $WRANGLER_BIN" >&2
    exit 1
  fi
else
  if ! command -v "$WRANGLER_BIN" >/dev/null 2>&1; then
    echo "Wrangler command not found in PATH: $WRANGLER_BIN" >&2
    exit 1
  fi
fi

if [[ -z "$PAGES_BRANCH" ]]; then
  echo "PAGES_BRANCH is required for Pages deployment verification." >&2
  exit 1
fi

if [[ -z "$PAGES_COMMIT_HASH" ]]; then
  echo "PAGES_COMMIT_HASH is required for Pages deployment verification." >&2
  exit 1
fi

if ! [[ "$PAGES_VERIFY_ATTEMPTS" =~ ^[0-9]+$ ]] || [[ "$PAGES_VERIFY_ATTEMPTS" -lt 1 ]]; then
  echo "PAGES_VERIFY_ATTEMPTS must be a positive integer (got: $PAGES_VERIFY_ATTEMPTS)." >&2
  exit 1
fi

if ! [[ "$PAGES_VERIFY_SLEEP_SECONDS" =~ ^[0-9]+$ ]] || [[ "$PAGES_VERIFY_SLEEP_SECONDS" -lt 1 ]]; then
  echo "PAGES_VERIFY_SLEEP_SECONDS must be a positive integer (got: $PAGES_VERIFY_SLEEP_SECONDS)." >&2
  exit 1
fi

case "$PAGES_VERIFY_MODE" in
  parallel|serial) ;;
  *)
    echo "PAGES_VERIFY_MODE must be 'parallel' or 'serial' (got: $PAGES_VERIFY_MODE)." >&2
    exit 1
    ;;
esac

PROJECT_ENV_VARS=(
  "PAGES_PROJECT_SITE"
  "PAGES_PROJECT_DOCS"
  "PAGES_PROJECT_WEB"
  "PAGES_PROJECT_DEMO"
)

PROJECT_NAMES=()
PROJECT_STAGES=()
PROJECT_ALIASES=()

for required_project in "${PROJECT_ENV_VARS[@]}"; do
  if [[ -z "${!required_project:-}" ]]; then
    echo "${required_project} is required for Pages deployment verification." >&2
    exit 1
  fi
  PROJECT_NAMES+=("${!required_project}")
  PROJECT_STAGES+=("unknown")
  PROJECT_ALIASES+=("")
done

if [[ "$PAGES_BRANCH" == "stable" ]]; then
  PAGES_ENVIRONMENT="production"
else
  PAGES_ENVIRONMENT="preview"
fi

query_project() {
  local project_name="$1"
  local deployments_json stage alias_line

  deployments_json="$($WRANGLER_BIN pages deployment list \
    --project-name "$project_name" \
    --environment "$PAGES_ENVIRONMENT" \
    --json)"

  stage="$(echo "$deployments_json" | jq -r \
    --arg branch "$PAGES_BRANCH" \
    --arg commit_hash "$PAGES_COMMIT_HASH" \
    '
    [ .[]
      | select(.deployment_trigger.metadata.branch == $branch)
      | select(.deployment_trigger.metadata.commit_hash == $commit_hash)
    ] as $matches
    | if ($matches | length) == 0
      then "missing"
      else ($matches[0].latest_stage.status // "unknown")
      end
    ')"

  alias_line="$(echo "$deployments_json" | jq -r \
    --arg branch "$PAGES_BRANCH" \
    --arg commit_hash "$PAGES_COMMIT_HASH" \
    '
    [ .[]
      | select(.deployment_trigger.metadata.branch == $branch)
      | select(.deployment_trigger.metadata.commit_hash == $commit_hash)
    ] as $matches
    | if ($matches | length) == 0
      then ""
      else ($matches[0].aliases | join(", "))
      end
    ')"

  printf '%s\t%s\n' "$stage" "$alias_line"
}

print_summary() {
  local prefix="$1"
  local index project_name stage aliases
  for index in "${!PROJECT_NAMES[@]}"; do
    project_name="${PROJECT_NAMES[$index]}"
    stage="${PROJECT_STAGES[$index]}"
    aliases="${PROJECT_ALIASES[$index]}"
    if [[ -n "$aliases" ]]; then
      echo "[verify-pages] ${prefix} project=${project_name} stage=${stage} aliases=${aliases}"
    else
      echo "[verify-pages] ${prefix} project=${project_name} stage=${stage}"
    fi
  done
}

verify_parallel() {
  local attempt index project_name result stage aliases

  for ((attempt = 1; attempt <= PAGES_VERIFY_ATTEMPTS; attempt++)); do
    local all_success=1
    local hard_fail=0

    for index in "${!PROJECT_NAMES[@]}"; do
      project_name="${PROJECT_NAMES[$index]}"

      if ! result="$(query_project "$project_name")"; then
        PROJECT_STAGES[$index]="query_error"
        PROJECT_ALIASES[$index]=""
        hard_fail=1
        continue
      fi

      stage="${result%%$'\t'*}"
      if [[ "$result" == *$'\t'* ]]; then
        aliases="${result#*$'\t'}"
      else
        aliases=""
      fi

      PROJECT_STAGES[$index]="$stage"
      PROJECT_ALIASES[$index]="$aliases"

      case "$stage" in
        success) ;;
        failure|canceled)
          hard_fail=1
          ;;
        *)
          all_success=0
          ;;
      esac
    done

    if [[ "$hard_fail" -eq 1 ]]; then
      echo "[verify-pages] hard failure while verifying Pages deployments." >&2
      print_summary "summary"
      return 1
    fi

    if [[ "$all_success" -eq 1 ]]; then
      echo "[verify-pages] all Pages targets verified for branch=${PAGES_BRANCH}, commit=${PAGES_COMMIT_HASH}"
      print_summary "summary"
      return 0
    fi

    echo "[verify-pages] waiting (attempt=${attempt}/${PAGES_VERIFY_ATTEMPTS})"
    print_summary "progress"
    if (( attempt < PAGES_VERIFY_ATTEMPTS )); then
      sleep "$PAGES_VERIFY_SLEEP_SECONDS"
    fi
  done

  echo "[verify-pages] did not reach success for all targets (branch=${PAGES_BRANCH}, commit=${PAGES_COMMIT_HASH})." >&2
  print_summary "summary"
  return 1
}

verify_project_serial() {
  local index="$1"
  local project_name="$2"
  local attempt result stage aliases

  for ((attempt = 1; attempt <= PAGES_VERIFY_ATTEMPTS; attempt++)); do
    if ! result="$(query_project "$project_name")"; then
      PROJECT_STAGES[$index]="query_error"
      PROJECT_ALIASES[$index]=""
      echo "[verify-pages] ${project_name}: query_error" >&2
      return 1
    fi

    stage="${result%%$'\t'*}"
    if [[ "$result" == *$'\t'* ]]; then
      aliases="${result#*$'\t'}"
    else
      aliases=""
    fi

    PROJECT_STAGES[$index]="$stage"
    PROJECT_ALIASES[$index]="$aliases"

    if [[ "$stage" == "success" ]]; then
      echo "[verify-pages] ${project_name}: success"
      return 0
    fi

    if [[ "$stage" == "failure" || "$stage" == "canceled" ]]; then
      echo "[verify-pages] ${project_name}: deployment stage=${stage}" >&2
      return 1
    fi

    echo "[verify-pages] ${project_name}: waiting (stage=${stage}, attempt=${attempt}/${PAGES_VERIFY_ATTEMPTS})"
    if (( attempt < PAGES_VERIFY_ATTEMPTS )); then
      sleep "$PAGES_VERIFY_SLEEP_SECONDS"
    fi
  done

  echo "[verify-pages] ${project_name}: did not reach success" >&2
  return 1
}

verify_serial() {
  local index project_name
  for index in "${!PROJECT_NAMES[@]}"; do
    project_name="${PROJECT_NAMES[$index]}"
    verify_project_serial "$index" "$project_name" || return 1
  done

  echo "[verify-pages] all Pages targets verified for branch=${PAGES_BRANCH}, commit=${PAGES_COMMIT_HASH}"
  print_summary "summary"
}

if [[ "$PAGES_VERIFY_MODE" == "parallel" ]]; then
  verify_parallel
else
  verify_serial
fi
