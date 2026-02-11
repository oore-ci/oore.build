#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT_DIR/scripts/launchd/com.oore.release-webhook.daemon.plist.template"
TARGET="/Library/LaunchDaemons/com.oore.release-webhook.plist"
LABEL="com.oore.release-webhook"

BUILD_USER="${OORE_BUILD_USER:-${SUDO_USER:-$(id -un)}}"
BUILD_HOME="$(dscl . -read "/Users/$BUILD_USER" NFSHomeDirectory 2>/dev/null | awk '{print $2}')"
if [[ -z "$BUILD_HOME" ]]; then
  BUILD_HOME="/Users/$BUILD_USER"
fi

ETC_DIR="/etc/oore"
ENV_FILE="$ETC_DIR/release-webhook.env"
USER_ENV_FILE="$BUILD_HOME/.oore/release-runner/webhook.env"
LOG_FILE="/var/log/oore-release-webhook.log"
STATE_DIR="$BUILD_HOME/.oore/release-runner"

log() {
  printf '[webhook-daemon-install] %s\n' "$*"
}

die() {
  printf '[webhook-daemon-install] ERROR: %s\n' "$*" >&2
  exit 1
}

require_root() {
  [[ "$EUID" -eq 0 ]] || die "Run with sudo: sudo make install-release-webhook-daemon"
}

require_root
[[ -f "$TEMPLATE" ]] || die "Template not found: $TEMPLATE"

mkdir -p "$ETC_DIR" "$(dirname "$TARGET")" "$STATE_DIR"
chown "$BUILD_USER":staff "$STATE_DIR"

if [[ ! -f "$ENV_FILE" ]]; then
  if [[ -f "$USER_ENV_FILE" ]] && grep -Eq '^(export[[:space:]]+)?OORE_WEBHOOK_SECRET=' "$USER_ENV_FILE"; then
    cp "$USER_ENV_FILE" "$ENV_FILE"
    chmod 600 "$ENV_FILE"
    chown root:wheel "$ENV_FILE"
    log "Copied webhook secret from $USER_ENV_FILE to $ENV_FILE"
  else
    cat > "$ENV_FILE" <<'EOF'
# Required: set your GitHub webhook secret.
# OORE_WEBHOOK_SECRET=replace-with-strong-random-secret
EOF
    chmod 600 "$ENV_FILE"
    chown root:wheel "$ENV_FILE"
    die "Created $ENV_FILE. Set OORE_WEBHOOK_SECRET, then rerun installer."
  fi
fi

if ! grep -Eq '^(export[[:space:]]+)?OORE_WEBHOOK_SECRET=' "$ENV_FILE"; then
  die "$ENV_FILE must include OORE_WEBHOOK_SECRET=..."
fi

touch "$LOG_FILE"
chmod 644 "$LOG_FILE"
chown root:wheel "$LOG_FILE"

sed \
  -e "s#__ROOT_DIR__#$ROOT_DIR#g" \
  -e "s#__BUILD_USER__#$BUILD_USER#g" \
  -e "s#__BUILD_HOME__#$BUILD_HOME#g" \
  "$TEMPLATE" > "$TARGET"

chmod 644 "$TARGET"
chown root:wheel "$TARGET"

if launchctl print "system/$LABEL" >/dev/null 2>&1; then
  launchctl bootout "system/$LABEL" >/dev/null 2>&1 || true
fi

launchctl bootstrap system "$TARGET"
launchctl enable "system/$LABEL"
launchctl kickstart -k "system/$LABEL"

log "Installed and started LaunchDaemon $LABEL as user $BUILD_USER"
log "Webhook health: curl -fsSL http://127.0.0.1:8789/healthz"
log "Logs: $LOG_FILE"
