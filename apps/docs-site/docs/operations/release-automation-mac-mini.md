---
status: implemented
---

# Release Automation on macOS

Use this flow with a dedicated macOS host (for example, a Mac mini) that builds release artifacts and uploads them to Cloudflare R2 behind `https://dl.oore.build`.

## Prerequisites

- macOS host with Xcode command line tools
- Rust toolchain installed
- `wrangler` installed and authenticated (`wrangler whoami`)
- R2 bucket and public domain already configured (example: `oore` and `dl.oore.build`)
- Repo checked out on the macOS host

## Webhook mode (recommended, LaunchDaemon)

This mode reacts to Git tag pushes immediately.

### 1) Configure webhook secret on Mac mini

```bash
sudo mkdir -p /etc/oore
sudo tee /etc/oore/release-webhook.env >/dev/null <<'EOF'
OORE_WEBHOOK_SECRET=replace-with-strong-random-secret
EOF
sudo chmod 600 /etc/oore/release-webhook.env
```

### 2) Install system daemon

```bash
sudo make install-release-webhook-daemon
```

### 3) Expose listener publicly (Cloudflare Tunnel)

Expose `http://127.0.0.1:8789` and map a hostname (for example `build-hook.oore.build`).

Use webhook URL:

- `https://build-hook.oore.build/github/webhook`

### 4) Configure GitHub webhook

- Event type: `Push`
- Content type: `application/json`
- Secret: same value as `OORE_WEBHOOK_SECRET`
- URL: `https://build-hook.oore.build/github/webhook`

### 5) Verify listener health

```bash
curl -fsSL http://127.0.0.1:8789/healthz
tail -f ~/Library/Logs/oore-release-webhook.log
```

On tag push `refs/tags/v*.*.*`, the listener triggers release build/upload.

### Check daemon status

```bash
sudo launchctl print system/com.oore.release-webhook | head -n 50
```

## LaunchAgent mode (fallback)

If you do not want system-level setup, you can still run the per-user LaunchAgent:

```bash
make install-release-webhook
```

Logs:

```bash
tail -f ~/Library/Logs/oore-release-webhook.log
```

## Polling mode (backup fallback)

If webhooks are unavailable, install the poller:

```bash
make install-release-poller
```

This `launchd` job:

- polls semver tags (`v*.*.*`) every 2 minutes
- builds and packages both macOS release artifacts
- uploads artifacts and checksums to R2
- updates `releases/latest.json` for `OORE_VERSION=latest`

## Manual release publish

To publish a specific tag immediately:

```bash
make release-local TAG=v0.2.0
```

Artifacts are uploaded to:

- `https://dl.oore.build/releases/v0.2.0/oore_0.2.0_darwin_arm64.tar.gz`
- `https://dl.oore.build/releases/v0.2.0/oore_0.2.0_darwin_x86_64.tar.gz`
- `https://dl.oore.build/releases/v0.2.0/oore_0.2.0_checksums.txt`
- `https://dl.oore.build/releases/latest.json`

## Environment variables

The release scripts support these overrides:

| Variable | Default | Description |
|---|---|---|
| `OORE_R2_BUCKET` | `oore` | R2 bucket name |
| `OORE_R2_PREFIX` | `releases` | Object key prefix in bucket |
| `OORE_RELEASE_BASE_URL` | `https://dl.oore.build/releases` | Public base URL used in manifests |
| `OORE_GIT_REMOTE` | `origin` | Git remote for tag polling/build source |
| `OORE_TAG_PATTERN` | `v*.*.*` | Tag filter for poller |
| `OORE_WEBHOOK_SECRET` | unset | Required for webhook signature verification |
| `OORE_PUBLISH_LATEST` | `1` | Upload `latest.json` after release |
| `OORE_SKIP_UPLOAD` | `0` | Build/package only, skip R2 upload |

## Verification

After publishing a tag:

```bash
curl -fsSL https://dl.oore.build/releases/latest.json
curl -I https://dl.oore.build/releases/v0.2.0/oore_0.2.0_darwin_arm64.tar.gz
curl -I https://oore.build/install
```

Then validate installer flow:

```bash
OORE_VERSION=latest curl -fsSL https://oore.build/install | bash
```
