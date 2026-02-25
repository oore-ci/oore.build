#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

WRANGLER_BIN="${WRANGLER:-./node_modules/.bin/wrangler}"
PAGES_BRANCH="${PAGES_BRANCH:-}"
PAGES_COMMIT_HASH="${PAGES_COMMIT_HASH:-}"
PAGES_COMMIT_SHORT="${PAGES_COMMIT_HASH:0:7}"
PAGES_COMMIT_MESSAGE="${PAGES_COMMIT_MESSAGE:-}"
PAGES_VERIFY_MODE="${PAGES_VERIFY_MODE:-parallel}"
PAGES_VERIFY_ENVIRONMENT="${PAGES_VERIFY_ENVIRONMENT:-auto}"
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

case "$PAGES_VERIFY_ENVIRONMENT" in
  auto|production|preview|all) ;;
  *)
    echo "PAGES_VERIFY_ENVIRONMENT must be one of auto|production|preview|all (got: $PAGES_VERIFY_ENVIRONMENT)." >&2
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
PROJECT_ENVIRONMENTS=()
PROJECT_MATCHED_BRANCHES=()
PROJECT_MATCHED_COMMITS=()

for required_project in "${PROJECT_ENV_VARS[@]}"; do
  if [[ -z "${!required_project:-}" ]]; then
    echo "${required_project} is required for Pages deployment verification." >&2
    exit 1
  fi
  PROJECT_NAMES+=("${!required_project}")
  PROJECT_STAGES+=("unknown")
  PROJECT_ALIASES+=("")
  PROJECT_ENVIRONMENTS+=("")
  PROJECT_MATCHED_BRANCHES+=("")
  PROJECT_MATCHED_COMMITS+=("")
done

environment_attempt_order() {
  if [[ "$PAGES_VERIFY_ENVIRONMENT" != "auto" ]]; then
    echo "$PAGES_VERIFY_ENVIRONMENT"
    return 0
  fi

  # Stable should normally map to production, but we fall back to preview/all
  # because Pages project production-branch settings can drift.
  if [[ "$PAGES_BRANCH" == "stable" ]]; then
    echo "production preview all"
  else
    echo "preview production all"
  fi
}

fetch_deployments_json() {
  local project_name="$1"
  local environment_name="$2"

  if [[ "$environment_name" == "all" ]]; then
    "$WRANGLER_BIN" pages deployment list \
      --project-name "$project_name" \
      --json
  else
    "$WRANGLER_BIN" pages deployment list \
      --project-name "$project_name" \
      --environment "$environment_name" \
      --json
  fi
}

select_matching_deployment() {
  local deployments_json="$1"

  jq -c \
    --arg branch "$PAGES_BRANCH" \
    --arg commit_hash "$PAGES_COMMIT_HASH" \
    --arg commit_short "$PAGES_COMMIT_SHORT" \
    --arg commit_message "$PAGES_COMMIT_MESSAGE" \
    '
    def meta: (.deployment_trigger.metadata // {});

    def branch_candidate($m):
      ($m.branch // $m.ref_name // $m.ref // "");

    def branch_match($m):
      ($m.branch // "") == $branch
      or ($m.ref_name // "") == $branch
      or ($m.ref // "") == $branch
      or ($m.ref // "") == ("refs/heads/" + $branch);

    def commit_candidate($m):
      ($m.commit_hash // $m.commitHash // $m.commit_sha // $m.sha // $m.commit // "");

    def commit_match($m):
      (commit_candidate($m) != "") and (
        commit_candidate($m) == $commit_hash
        or commit_candidate($m) == $commit_short
        or ($commit_hash | startswith(commit_candidate($m)))
        or (commit_candidate($m) | startswith($commit_short))
      );

    def message_match($m):
      ($commit_message != "") and (
        ($m.commit_message // $m.commitMessage // $m.message // "") == $commit_message
      );

    [
      .[] as $d
      | ($d | meta) as $m
      | select(branch_match($m))
      | select(commit_match($m) or message_match($m))
      | {
          stage: ($d.latest_stage.status // "unknown"),
          aliases: (($d.aliases // []) | join(", ")),
          environment: ($d.environment // ""),
          matched_branch: branch_candidate($m),
          matched_commit: commit_candidate($m)
        }
    ][0] // empty
    ' <<<"$deployments_json"
}

query_project() {
  local project_name="$1"
  local env_name deployments_json match_json
  local env_attempts

  env_attempts="$(environment_attempt_order)"

  for env_name in $env_attempts; do
    if ! deployments_json="$(fetch_deployments_json "$project_name" "$env_name")"; then
      return 1
    fi

    if ! match_json="$(select_matching_deployment "$deployments_json")"; then
      return 1
    fi

    if [[ -n "$match_json" ]]; then
      if [[ "$(jq -r '.environment // ""' <<<"$match_json")" == "" ]]; then
        match_json="$(jq -c --arg env "$env_name" '.environment = $env' <<<"$match_json")"
      fi
      printf '%s\n' "$match_json"
      return 0
    fi
  done

  printf '{"stage":"missing","aliases":"","environment":"","matched_branch":"","matched_commit":""}\n'
}

print_summary() {
  local prefix="$1"
  local index project_name stage aliases environment matched_branch matched_commit
  for index in "${!PROJECT_NAMES[@]}"; do
    project_name="${PROJECT_NAMES[$index]}"
    stage="${PROJECT_STAGES[$index]}"
    aliases="${PROJECT_ALIASES[$index]}"
    environment="${PROJECT_ENVIRONMENTS[$index]}"
    matched_branch="${PROJECT_MATCHED_BRANCHES[$index]}"
    matched_commit="${PROJECT_MATCHED_COMMITS[$index]}"

    echo "[verify-pages] ${prefix} project=${project_name} stage=${stage} env=${environment:-unknown} matched_branch=${matched_branch:-none} matched_commit=${matched_commit:-none} aliases=${aliases:-none}"
  done
}

verify_parallel() {
  local attempt index project_name result stage aliases environment matched_branch matched_commit

  for ((attempt = 1; attempt <= PAGES_VERIFY_ATTEMPTS; attempt++)); do
    local all_success=1
    local hard_fail=0

    for index in "${!PROJECT_NAMES[@]}"; do
      project_name="${PROJECT_NAMES[$index]}"

      if ! result="$(query_project "$project_name")"; then
        PROJECT_STAGES[$index]="query_error"
        PROJECT_ALIASES[$index]=""
        PROJECT_ENVIRONMENTS[$index]=""
        PROJECT_MATCHED_BRANCHES[$index]=""
        PROJECT_MATCHED_COMMITS[$index]=""
        hard_fail=1
        continue
      fi

      stage="$(jq -r '.stage // "unknown"' <<<"$result")"
      aliases="$(jq -r '.aliases // ""' <<<"$result")"
      environment="$(jq -r '.environment // ""' <<<"$result")"
      matched_branch="$(jq -r '.matched_branch // ""' <<<"$result")"
      matched_commit="$(jq -r '.matched_commit // ""' <<<"$result")"

      PROJECT_STAGES[$index]="$stage"
      PROJECT_ALIASES[$index]="$aliases"
      PROJECT_ENVIRONMENTS[$index]="$environment"
      PROJECT_MATCHED_BRANCHES[$index]="$matched_branch"
      PROJECT_MATCHED_COMMITS[$index]="$matched_commit"

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
  local attempt result stage aliases environment matched_branch matched_commit

  for ((attempt = 1; attempt <= PAGES_VERIFY_ATTEMPTS; attempt++)); do
    if ! result="$(query_project "$project_name")"; then
      PROJECT_STAGES[$index]="query_error"
      PROJECT_ALIASES[$index]=""
      PROJECT_ENVIRONMENTS[$index]=""
      PROJECT_MATCHED_BRANCHES[$index]=""
      PROJECT_MATCHED_COMMITS[$index]=""
      echo "[verify-pages] ${project_name}: query_error" >&2
      return 1
    fi

    stage="$(jq -r '.stage // "unknown"' <<<"$result")"
    aliases="$(jq -r '.aliases // ""' <<<"$result")"
    environment="$(jq -r '.environment // ""' <<<"$result")"
    matched_branch="$(jq -r '.matched_branch // ""' <<<"$result")"
    matched_commit="$(jq -r '.matched_commit // ""' <<<"$result")"

    PROJECT_STAGES[$index]="$stage"
    PROJECT_ALIASES[$index]="$aliases"
    PROJECT_ENVIRONMENTS[$index]="$environment"
    PROJECT_MATCHED_BRANCHES[$index]="$matched_branch"
    PROJECT_MATCHED_COMMITS[$index]="$matched_commit"

    if [[ "$stage" == "success" ]]; then
      echo "[verify-pages] ${project_name}: success env=${environment:-unknown} matched_branch=${matched_branch:-none} matched_commit=${matched_commit:-none}"
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

echo "[verify-pages] mode=${PAGES_VERIFY_MODE} env_mode=${PAGES_VERIFY_ENVIRONMENT} branch=${PAGES_BRANCH} commit=${PAGES_COMMIT_HASH}"

if [[ "$PAGES_VERIFY_MODE" == "parallel" ]]; then
  verify_parallel
else
  verify_serial
fi
