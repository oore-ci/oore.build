---
status: implemented
description: "Install Oore CI daemon and CLI binaries on macOS with a single command."
---

# Install Oore CI

This page walks you through installing prebuilt backend binaries from GitHub Releases.

## What you need

- macOS host (V1 backend runtime target)
- `curl`
- Internet access to GitHub (`github.com`), `ci.oore.build`, and `docs.oore.build`

## Install (latest release)

```bash
curl -fsSL https://oore.build/install | bash
```

## Install by channel (stable/beta/alpha)

The installer supports release channels:

- `stable` (default): latest non-prerelease GitHub Release
- `beta`: latest `vX.Y.Z-beta.N` prerelease
- `alpha`: latest `vX.Y.Z-alpha.N` prerelease

```bash
# stable (default)
curl -fsSL https://oore.build/install | bash

# beta
curl -fsSL https://oore.build/install | OORE_CHANNEL=beta bash

# alpha
curl -fsSL https://oore.build/install | OORE_CHANNEL=alpha bash
```

`OORE_VERSION` (pinned tag/version) always overrides channel selection.

The installer:

- Detects your architecture (`arm64` or `x86_64`)
- Downloads the matching release tarball
- Verifies SHA-256 checksums
- Installs `oored`, `oore`, and `oore-web` under `~/.oore/bin`
- Installs prebuilt local web assets under `~/.oore/web-dist`
- Prompts for optional first-run actions (start daemon, generate setup token, open links)
- For localhost backends, asks whether you plan to expose HTTPS publicly and can start/auto-start local web UI for you

`oored` stores its local encryption key in a file under the daemon data directory
(for example `~/Library/Application Support/oore/encryption.key`) and applies `0600` permissions.

## Install a pinned version

```bash
curl -fsSL https://oore.build/install | OORE_VERSION=v0.2.0 bash
```

## Non-interactive mode (automation)

```bash
curl -fsSL https://oore.build/install | OORE_NONINTERACTIVE=1 OORE_START_DAEMON=true bash
```

If `OORE_NONINTERACTIVE=1` and `OORE_START_DAEMON` is not set, daemon startup is skipped.

## Frontend-only install

Use frontend-only mode when `oored` runs on a Mac host but the browser-facing web UI runs on a separate Linux or macOS machine.

Example for an Ubuntu frontend host that reaches the Mac daemon through NetBird:

```bash
curl -fsSL https://oore.build/install | \
  OORE_INSTALL_MODE=frontend \
  OORE_WEB_BACKEND_URL=http://100.64.10.20:8787 \
  OORE_LOCAL_WEB_LISTEN=127.0.0.1:4173 \
  OORE_LOCAL_WEB_MODE=login \
  OORE_NONINTERACTIVE=1 \
  bash
```

Frontend-only mode:

- Downloads `oore-web` and the prebuilt `web-dist` assets only.
- Supports Linux and macOS release assets.
- Does not install or start `oored`, `oore`, or the embedded runner.
- Proxies `/v1/*` and `/healthz` from the frontend host to `OORE_WEB_BACKEND_URL`.
- Uses a systemd user service on Linux when `OORE_LOCAL_WEB_MODE=login`.

For a Linux user service to survive logout and reboot, enable lingering for the service user:

```bash
sudo loginctl enable-linger "$USER"
```

Put Warpgate and your HTTPS reverse proxy in front of the frontend host, then proxy traffic to `http://127.0.0.1:4173`.
In the web UI, add an instance with **Backend URL** left empty so API calls use the same HTTPS origin and flow through the frontend proxy.

## Verify installation

```bash
oored version
oore version
```

If `oore`/`oored` are not found, open a new terminal (so your shell picks up PATH changes) or use the full path under `~/.oore/bin`.

## Update (self-update)

`oore update` downloads the latest release for your installed channel and updates binaries in-place.

```bash
oore update --check
oore update
```

Override channel explicitly:

```bash
oore update --channel alpha
```

## Next step: choose setup path

Before using hosted UI, ensure your backend is HTTPS-reachable from the browser.
`https://ci.oore.build` cannot call `http://127.0.0.1:*` directly.

If your backend is local-only:

- Use CLI setup: `oore setup`, or
- expose it via tunnel first (for example `cloudflared tunnel --url http://127.0.0.1:8787`), or
- run local frontend: `oore-web --backend-url http://127.0.0.1:8787`.
  In the local web UI, add an instance and leave **Backend URL** empty so requests use the built-in proxy.

For hosted setup, open [ci.oore.build](https://ci.oore.build), add your backend URL, and complete setup.  
For local setup UI, open `http://127.0.0.1:4173/setup` (or your configured `OORE_LOCAL_WEB_LISTEN` address).

Continue with [Hosted UI Onboarding](/getting-started/hosted-ui-onboarding).

## Installer environment variables

| Variable | Default | Description |
|---|---|---|
| `OORE_VERSION` | `latest` | Release selector (`latest` or tag like `v0.2.0`) |
| `OORE_CHANNEL` | `stable` | Channel selector when `OORE_VERSION=latest`: `stable`, `beta`, or `alpha` |
| `OORE_INSTALL_MODE` | `full` | Install `full` macOS backend/CLI/frontend bundle, or `frontend` web-only bundle |
| `OORE_INSTALL_ROOT` | `~/.oore` | Installation directory |
| `OORE_GITHUB_REPO` | `devaryakjha/oore.build` | GitHub repository used to resolve `latest` and download assets |
| `OORE_RELEASE_BASE_URL` | `https://github.com/<repo>/releases/download` | Base URL that contains `<tag>/` release assets |
| `OORE_RELEASE_MANIFEST_URL` | `https://api.github.com/repos/<repo>/releases/latest` | Metadata URL used when `OORE_VERSION=latest` |
| `OORE_RELEASES_LIST_URL` | `https://api.github.com/repos/<repo>/releases?per_page=100` | Release list URL used when `OORE_VERSION=latest` and `OORE_CHANNEL` is `alpha` or `beta` |
| `OORE_NONINTERACTIVE` | `0` | Disable prompts when set to `1` |
| `OORE_START_DAEMON` | unset | Non-interactive daemon startup behavior (`true` or `false`) |
| `OORE_DAEMON_URL` | `http://127.0.0.1:8787` | Daemon URL used by full-mode setup helpers |
| `OORE_WEB_BACKEND_URL` | `OORE_DAEMON_URL` | Backend URL proxied by `oore-web`, useful for frontend-only hosts |
| `OORE_LOCAL_WEB_MODE` | unset | Non-interactive local web behavior for localhost backends: `off`, `run`, or `login` (launch-at-login) |
| `OORE_LOCAL_WEB_LISTEN` | `127.0.0.1:4173` | Bind address for `oore-web` |

## Troubleshooting

### Unsupported architecture

Full backend install currently supports macOS `arm64` and `x86_64`. Frontend-only install supports Linux and macOS `arm64` / `x86_64` release assets.

### Checksum mismatch

The installer exits before installing binaries if checksums do not match. Re-run once to rule out transient download issues. If it persists, do not continue and verify release assets on the GitHub Release for that tag.

### Daemon startup failed

Check logs:

```bash
cat ~/.oore/logs/oored.log
```

Then run diagnostics:

```bash
oore doctor
```

### Permission denied under `~/.oore`

If installer output shows `Permission denied` creating `~/.oore/bin` or `~/.oore/logs`,
the install root is likely owned by `root` from a prior system-level setup.

Fix ownership and rerun installer:

```bash
sudo chown -R "$USER":staff ~/.oore
curl -fsSL https://oore.build/install | bash
```

Or install to a different user-owned root:

```bash
curl -fsSL https://oore.build/install | OORE_INSTALL_ROOT="$HOME/.oore-user" bash
```
