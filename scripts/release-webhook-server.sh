#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_DIR="${OORE_RELEASE_STATE_DIR:-$HOME/.oore/release-runner}"
ENV_FILE="${OORE_WEBHOOK_ENV_FILE:-$STATE_DIR/webhook.env}"

mkdir -p "$STATE_DIR"

if [[ -f "$ENV_FILE" ]]; then
  # Export sourced variables so they are visible to the embedded Python process.
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  set +a
fi

: "${OORE_WEBHOOK_SECRET:?OORE_WEBHOOK_SECRET is required (set in environment or $ENV_FILE)}"

OORE_WEBHOOK_HOST="${OORE_WEBHOOK_HOST:-127.0.0.1}"
OORE_WEBHOOK_PORT="${OORE_WEBHOOK_PORT:-8789}"
OORE_WEBHOOK_PATH="${OORE_WEBHOOK_PATH:-/github/webhook}"

exec python3 - "$ROOT_DIR" "$STATE_DIR" "$OORE_WEBHOOK_HOST" "$OORE_WEBHOOK_PORT" "$OORE_WEBHOOK_PATH" <<'PY'
import hashlib
import hmac
import json
import os
import subprocess
import sys
import threading
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

root_dir = sys.argv[1]
state_dir = sys.argv[2]
host = sys.argv[3]
port = int(sys.argv[4])
path = sys.argv[5]
secret = os.environ["OORE_WEBHOOK_SECRET"].encode("utf-8")
release_script = os.path.join(root_dir, "scripts", "release-local.sh")
state_file = os.path.join(state_dir, "published-tags.txt")
in_progress = set()
lock = threading.Lock()

os.makedirs(state_dir, exist_ok=True)
if not os.path.exists(state_file):
    with open(state_file, "a", encoding="utf-8"):
        pass


def log(msg: str) -> None:
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    print(f"[release-webhook] {ts} {msg}", flush=True)


def is_processed(tag: str) -> bool:
    with open(state_file, "r", encoding="utf-8") as f:
        return any(line.strip() == tag for line in f)


def mark_processed(tag: str) -> None:
    with open(state_file, "a", encoding="utf-8") as f:
        f.write(tag + "\n")


def run_release(tag: str) -> None:
    try:
        log(f"starting release publish for {tag}")
        result = subprocess.run(
            [release_script, tag],
            cwd=root_dir,
            check=False,
        )
        if result.returncode != 0:
            log(f"release failed for {tag} (exit={result.returncode})")
            return
        mark_processed(tag)
        log(f"release completed for {tag}")
    finally:
        with lock:
            in_progress.discard(tag)


class Handler(BaseHTTPRequestHandler):
    server_version = "oore-release-webhook/1.0"

    def _json(self, status: int, payload: dict) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt: str, *args) -> None:
        log(fmt % args)

    def do_GET(self) -> None:
        if self.path == "/healthz":
            self._json(200, {"ok": True})
            return
        self._json(404, {"error": "not_found"})

    def do_POST(self) -> None:
        if self.path != path:
            self._json(404, {"error": "not_found"})
            return

        length = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(length)

        header_sig = self.headers.get("X-Hub-Signature-256", "")
        event = self.headers.get("X-GitHub-Event", "")

        expected = "sha256=" + hmac.new(secret, body, hashlib.sha256).hexdigest()
        if not hmac.compare_digest(expected, header_sig):
            self._json(401, {"error": "invalid_signature"})
            return

        if event == "ping":
            self._json(200, {"ok": True, "event": "ping"})
            return

        if event != "push":
            self._json(200, {"ok": True, "ignored": "event_not_supported", "event": event})
            return

        try:
            payload = json.loads(body.decode("utf-8"))
        except json.JSONDecodeError:
            self._json(400, {"error": "invalid_json"})
            return

        ref = payload.get("ref", "")
        if not ref.startswith("refs/tags/v"):
            self._json(200, {"ok": True, "ignored": "not_semver_tag_push", "ref": ref})
            return

        tag = ref.split("/")[-1]
        with lock:
            if tag in in_progress:
                self._json(200, {"ok": True, "status": "already_in_progress", "tag": tag})
                return
            if is_processed(tag):
                self._json(200, {"ok": True, "status": "already_processed", "tag": tag})
                return
            in_progress.add(tag)

        threading.Thread(target=run_release, args=(tag,), daemon=True).start()
        self._json(202, {"ok": True, "status": "release_started", "tag": tag})


def main() -> None:
    server = ThreadingHTTPServer((host, port), Handler)
    log(f"listening on http://{host}:{port}{path}")
    server.serve_forever()


if __name__ == "__main__":
    main()
PY
