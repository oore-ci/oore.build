#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT_DIR/scripts/launchd/com.oore.release-poller.plist.template"
TARGET="$HOME/Library/LaunchAgents/com.oore.release-poller.plist"
LABEL="com.oore.release-poller"

log() {
  printf '[launchd-install] %s\n' "$*"
}

die() {
  printf '[launchd-install] ERROR: %s\n' "$*" >&2
  exit 1
}

[[ -f "$TEMPLATE" ]] || die "Template not found: $TEMPLATE"
mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"

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
log "Logs: $HOME/Library/Logs/oore-release-poller.log"
