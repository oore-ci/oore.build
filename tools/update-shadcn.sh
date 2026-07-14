#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:---diff}"

COMPONENTS=(
  alert-dialog pagination tabs card input-group sheet scroll-area label sonner
  empty tooltip alert breadcrumb command avatar kbd dialog badge sidebar table
  separator button checkbox spinner collapsible dropdown-menu select textarea
  input skeleton form
)

case "$MODE" in
  --diff)
    EXTRA_ARGS=(--diff)
    ;;
  --apply)
    EXTRA_ARGS=()
    ;;
  *)
    echo "Usage: bash tools/update-shadcn.sh [--diff|--apply]" >&2
    exit 2
    ;;
esac

cd "$ROOT_DIR/apps/web"
echo "[shadcn] CLI: $(bunx --bun shadcn@latest --version)"
echo "[shadcn] Mode: ${MODE#--}"
bunx --bun shadcn@latest add "${COMPONENTS[@]}" "${EXTRA_ARGS[@]}"

if [[ "$MODE" == "--apply" ]]; then
  cat <<'EOF'
[shadcn] Registry changes applied interactively.
[shadcn] Preserve the documented Oore extensions in Badge, AlertDialogAction,
[shadcn] ScrollArea, SidebarInset, Table, Spinner, and use-mobile, then run:
  make test-web
  make validate
EOF
fi
