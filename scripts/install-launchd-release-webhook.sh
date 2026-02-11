#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT_DIR/scripts/launchd/com.oore.release-webhook.plist.template"
TARGET="$HOME/Library/LaunchAgents/com.oore.release-webhook.plist"
LABEL="com.oore.release-webhook"
ENV_FILE="$HOME/.oore/release-runner/webhook.env"

log() {
  printf '[webhook-install] %s\n' "$*"
}

die() {
  printf '[webhook-install] ERROR: %s\n' "$*" >&2
  exit 1
}

[[ -f "$TEMPLATE" ]] || die "Template not found: $TEMPLATE"
mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs" "$HOME/.oore/release-runner"

if [[ ! -f "$ENV_FILE" ]]; then
  cat > "$ENV_FILE" <<'EOF'
# Required: set your GitHub webhook secret before starting service.
# OORE_WEBHOOK_SECRET=replace-with-strong-random-secret
EOF
  chmod 600 "$ENV_FILE"
  die "Created $ENV_FILE. Set OORE_WEBHOOK_SECRET, then rerun this installer."
fi

if ! grep -Eq '^(export[[:space:]]+)?OORE_WEBHOOK_SECRET=' "$ENV_FILE"; then
  die "$ENV_FILE must include OORE_WEBHOOK_SECRET=..."
fi

sed \
  -e "s#__ROOT_DIR__#$ROOT_DIR#g" \
  -e "s#__HOME__#$HOME#g" \
  "$TEMPLATE" > "$TARGET"

chmod 644 "$TARGET"

if launchctl print "gui/$UID/$LABEL" >/dev/null 2>&1; then
  launchctl bootout "gui/$UID/$LABEL" >/dev/null 2>&1 || true
fi

launchctl bootstrap "gui/$UID" "$TARGET"
launchctl enable "gui/$UID/$LABEL"
launchctl kickstart -k "gui/$UID/$LABEL"

log "Installed and started $LABEL"
log "Webhook health: curl -fsSL http://127.0.0.1:8789/healthz"
log "Logs: $HOME/Library/Logs/oore-release-webhook.log"
