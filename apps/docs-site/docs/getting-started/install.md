---
status: implemented
description: "Install Oore CI backend or frontend roles with a single command."
---

# Install Oore CI

This page walks you through installing prebuilt Oore CI release assets from GitHub Releases.

## What you need

- macOS host for backend mode (V1 backend runtime target)
- Linux or macOS host for frontend-only mode
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
curl -fsSL https://beta.oore.pages.dev/install | OORE_CHANNEL=beta bash

# alpha
curl -fsSL https://alpha.oore.pages.dev/install | OORE_CHANNEL=alpha bash
```

`OORE_VERSION` (pinned tag/version) always overrides channel selection. Use the matching channel installer endpoint when testing prerelease installer behavior; `https://oore.build/install` is the stable production installer.

## Install modes

The installer is role-based:

- `auto`: macOS prompts for a role in interactive shells; Linux defaults to frontend-only mode.
- `all`: installs the daemon, CLI, embedded runner, `oore-web`, and local web assets on one macOS host.
- `backend`: installs only the daemon, CLI, and embedded runner on a macOS backend host.
- `frontend`: installs only `oore-web` and static frontend assets on a Linux or macOS frontend host.

`full` is still accepted as a compatibility alias for `all`, but new docs and scripts should use role names. A future runner-only mode will be added separately when external runner packaging is ready.

The installer:

- Detects your architecture (`arm64` or `x86_64`)
- Prompts for the macOS role and chooses frontend-only mode on Linux when `OORE_INSTALL_MODE=auto`
- Downloads the matching release tarball
- Verifies SHA-256 checksums
- Installs the binaries and web assets required by the selected role
- Prompts for role-specific configuration: daemon listen address, public URL, launchd service, frontend backend URL, loopback listen address, and frontend autostart where relevant

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

## Backend-only install

Use backend mode when another host will serve the web UI, or when you want daemon/CLI setup without local frontend assets.

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=backend \
  bash
```

For non-interactive backend setup on a private interface:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=backend \
  OORE_DAEMON_LISTEN=10.0.0.20:8787 \
  OORE_SETUP_OWNER_EMAIL=owner@example.com \
  OORE_SETUP_PROXY_PRESET=generic \
  OORE_INSTALL_DAEMON_SERVICE=true \
  OORE_NONINTERACTIVE=1 \
  bash
```

For split deployments where the browser reaches the API through `oore-web` on the frontend host, leave `OORE_PUBLIC_URL` / `OORE_CORS_ORIGINS` unset during backend install. The installer can prefill Trusted Proxy setup values, but it does not block the terminal waiting for the browser wizard to finish.

## Frontend-only install

Use frontend-only mode when `oored` runs on one host but the browser-facing web UI runs on a separate Linux or macOS machine. On Linux, `OORE_INSTALL_MODE=auto` selects frontend-only mode automatically.

For an interactive Linux frontend install, run:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | OORE_CHANNEL=alpha bash
```

The installer asks for the backend daemon URL, keeps `oore-web` on loopback by default, can install a systemd user service, and can enable lingering so the service survives logout/reboot.

Example for a frontend host that reaches the backend over a private network:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=frontend \
  OORE_WEB_BACKEND_URL=http://10.0.0.20:8787 \
  OORE_LOCAL_WEB_LISTEN=127.0.0.1:4173 \
  OORE_LOCAL_WEB_MODE=login \
  OORE_ENABLE_LINGER=true \
  OORE_NONINTERACTIVE=1 \
  bash
```

Frontend-only mode:

- Downloads `oore-web` and the prebuilt `web-dist` assets only.
- Supports Linux and macOS release assets.
- Does not install or start `oored`, `oore`, or the embedded runner.
- Proxies `/v1/*` and `/healthz` from the frontend host to `OORE_WEB_BACKEND_URL`.
- Uses a systemd user service on Linux when `OORE_LOCAL_WEB_MODE=login`.
- Refuses non-interactive frontend-only installs unless `OORE_WEB_BACKEND_URL` or `OORE_DAEMON_URL` was explicitly provided.
- Can be updated later with `oore-web update` or checked with `oore-web update --check`.

For a Linux user service to survive logout and reboot, enable lingering for the service user:

```bash
sudo loginctl enable-linger "$USER"
```

Put your HTTPS reverse proxy in front of the frontend host, then proxy traffic to `http://127.0.0.1:4173`.
In the web UI, add an instance with **Backend URL** left empty so API calls use the same HTTPS origin and flow through the frontend proxy.

## Verify installation

```bash
oored version
oore version
```

If `oore`/`oored` are not found, open a new terminal (so your shell picks up PATH changes) or use the full path under `~/.oore/bin`.

## Run the daemon as a service

For a persistent local daemon, install `oored` as a macOS launchd user service. Interactive installs ask whether to do this for you.

```bash
oored install-service --listen 127.0.0.1:8787
```

The service keeps `oored` running across login sessions and writes logs to
`~/.oore/logs/oored.log`. To pass deployment-specific settings into launchd,
repeat `--env KEY=VALUE`:

```bash
oored install-service \
  --listen 127.0.0.1:8787 \
  --env OORE_PUBLIC_URL=https://ci.mycompany.com \
  --env OORE_CORS_ORIGINS=https://ci.mycompany.com
```

Remove the service without deleting data:

```bash
oored uninstall-service
```

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
| `OORE_INSTALL_MODE` | `auto` | Install mode: `auto`, `all`, `backend`, or `frontend`; `full` is accepted as a legacy alias for `all` |
| `OORE_INSTALL_ROOT` | `~/.oore` | Installation directory |
| `OORE_GITHUB_REPO` | `devaryakjha/oore.build` | GitHub repository used to resolve `latest` and download assets |
| `OORE_RELEASE_BASE_URL` | `https://github.com/<repo>/releases/download` | Base URL that contains `<tag>/` release assets |
| `OORE_RELEASE_MANIFEST_URL` | `https://api.github.com/repos/<repo>/releases/latest` | Metadata URL used when `OORE_VERSION=latest` |
| `OORE_RELEASES_LIST_URL` | `https://api.github.com/repos/<repo>/releases?per_page=100` | Release list URL used when `OORE_VERSION=latest` and `OORE_CHANNEL` is `alpha` or `beta` |
| `OORE_NONINTERACTIVE` | `0` | Disable prompts when set to `1` |
| `OORE_DAEMON_LISTEN` | from `OORE_DAEMON_URL` | Daemon listen address for `all` and `backend` installs, for example `127.0.0.1:8787` for same-host reverse proxy or `10.0.0.20:8787` for a private frontend host |
| `OORE_START_DAEMON` | unset | Non-interactive daemon startup behavior (`true` or `false`) |
| `OORE_INSTALL_DAEMON_SERVICE` | unset | Non-interactive launchd service install/start behavior for `all` and `backend` macOS installs (`true` or `false`) |
| `OORE_PUBLIC_URL` | unset | Browser-visible HTTPS URL passed to the daemon service as External Access fallback |
| `OORE_CORS_ORIGINS` | `OORE_PUBLIC_URL` when set | Comma-separated allowed browser origins passed to the daemon service |
| `OORE_DAEMON_URL` | `http://127.0.0.1:8787` | Daemon URL used by backend setup helpers |
| `OORE_WEB_BACKEND_URL` | `OORE_DAEMON_URL` | Backend URL proxied by `oore-web`, useful for frontend-only hosts |
| `OORE_SETUP_OWNER_EMAIL` | unset | Initial owner email to prefill in Trusted Proxy setup |
| `OORE_SETUP_PROXY_PRESET` | `generic` | Trusted Proxy setup prefill: `generic`, `warpgate`, or `custom` |
| `OORE_SETUP_USER_EMAIL_HEADER` | unset | Custom Trusted Proxy email header when `OORE_SETUP_PROXY_PRESET=custom` |
| `OORE_LOCAL_WEB_MODE` | unset | Non-interactive local web behavior for localhost backends: `off`, `run`, or `login` (launch-at-login) |
| `OORE_LOCAL_WEB_LISTEN` | `127.0.0.1:4173` | Bind address for `oore-web` |
| `OORE_ENABLE_LINGER` | unset | Enable systemd lingering for Linux frontend service installs (`true` or `false`) |

## Troubleshooting

### Unsupported architecture

All-in-one and backend installs currently support macOS `arm64` and `x86_64`. Frontend-only install supports Linux and macOS `arm64` / `x86_64` release assets.

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
