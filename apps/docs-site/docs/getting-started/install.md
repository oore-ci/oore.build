---
status: implemented
description: 'Install Oore CI backend or frontend roles with a single command.'
---

# Install Oore CI

This page walks you through installing prebuilt Oore CI release assets from GitHub Releases.
Installation puts the daemon, CLI, and/or frontend launcher on disk. First-run setup is a separate step owned by the backend daemon; the hosted and self-hosted web UIs are clients for that backend setup flow.

## What you need

- macOS host for backend mode (V1 backend runtime target)
- Linux or macOS host for frontend-only mode
- `curl`
- Internet access to GitHub (`github.com`) and the installer endpoint for your channel
- Access to `ci.oore.build` only if you plan to use the hosted UI

## Install on one Mac (default)

```bash
curl -fsSL https://oore.build/install | bash
```

On macOS, this is the local-first path: it installs the daemon, CLI, runner binary, and local web UI; keeps the daemon and web services on loopback; enables launch-at-login; starts them; and opens `http://127.0.0.1:4173`.
It uses loopback local login, so it does not generate a bootstrap token or send you to `/setup`.

After signing in, register the Direct macOS runner once and install its separate
login-session service. Then enable Direct runner execution in **Settings >
Preferences** and approve each repository in **Settings > Sources**. These trust
controls default off after both fresh installs and upgrades.

Use `--no-open` to suppress the browser. Non-interactive installs do not open a browser unless you explicitly set `OORE_OPEN_BROWSER=true`.

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

## Advanced topology install modes

For split, remote, or frontend-only deployments, keep the existing topology wizard behind `--advanced`:

```bash
curl -fsSL https://oore.build/install | bash -s -- --advanced
```

The advanced installer is role-based:

- `auto`: macOS prompts for a role in interactive shells; Linux defaults to frontend-only mode.
- `all`: installs the daemon, CLI (including runner commands), `oore-web`, and local web assets on one macOS host.
- `backend`: installs only the daemon and CLI (including runner commands) on a macOS backend host.
- `frontend`: installs only `oore-web` and static frontend assets on a Linux or macOS frontend host.

`full` is still accepted as a compatibility alias for `all`, but new docs and scripts should use role names. The Direct macOS runner is registered and managed through `oore runner`; there is no embedded execution mode.

## Deployment shapes and setup modes

Install roles describe where binaries run. Setup modes describe how the backend will authenticate users after first-run setup.

| Shape                               | Install role                                      | Setup mode                                      | Use when                                                                                                                 |
| ----------------------------------- | ------------------------------------------------- | ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Single Mac, local evaluation        | `all`                                             | `Local Only`                                    | You only access the daemon from loopback on the same machine. No OIDC, proxy, or local passwords.                        |
| Single Mac, remote browser access   | `all` or `backend`                                | `Remote OIDC`                                   | Users reach the backend through HTTPS and sign in with any OIDC-compatible identity provider.                            |
| Single Mac behind an identity proxy | `all` or `backend`                                | `Remote Trusted Proxy`                          | Your proxy already authenticates users and forwards a trusted identity header.                                           |
| Split frontend/backend              | `backend` on macOS plus `frontend` on Linux/macOS | Usually `Remote Trusted Proxy` or `Remote OIDC` | A browser-facing `oore-web` host proxies API calls to the macOS backend over a controlled network path.                  |
| Hosted UI                           | `backend` on macOS                                | `Remote OIDC` or `Remote Trusted Proxy`         | `ci.oore.build` serves only the frontend app; your macOS backend still owns setup, auth, data, builds, and signing keys. |

The advanced installer:

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

For split deployments where the browser reaches the API through `oore-web` on the frontend host, leave `OORE_PUBLIC_URL` / `OORE_CORS_ORIGINS` unset during backend install unless browsers will call `oored` directly. When you provide `OORE_SETUP_OWNER_EMAIL`, the installer initializes Remote Trusted Proxy setup on the backend host and writes the backend shared secret to `~/.oore/trusted-proxy-shared-secret` if you did not provide one.

## Frontend-only install

Use frontend-only mode when `oored` runs on one host but the browser-facing web UI runs on a separate Linux or macOS machine. On Linux, `OORE_INSTALL_MODE=auto` selects frontend-only mode automatically.

For an interactive Linux frontend install, run:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | OORE_CHANNEL=alpha bash
```

The installer asks for the backend daemon URL, keeps `oore-web` on loopback by default, can install a systemd user service, and can enable lingering so the service survives logout/reboot. For remote HTTP backends or non-loopback HTTP listeners, the interactive flow requires an explicit confirmation that the corresponding encrypted transport is already configured; non-interactive installs use the transport-protection variables shown below.

Example for a frontend host that reaches the backend over a private network:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=frontend \
  OORE_WEB_BACKEND_URL=http://10.0.0.20:8787 \
  OORE_WEB_BACKEND_TRANSPORT_PROTECTED=true \
  OORE_LOCAL_WEB_LISTEN=127.0.0.1:4173 \
  OORE_LOCAL_WEB_MODE=login \
  OORE_ENABLE_LINGER=true \
  OORE_NONINTERACTIVE=1 \
  bash
```

For a Trusted Proxy frontend, prefer pairing instead of copying the backend proof manually. On the ready backend host, run `oore frontend invite`, then provide the emitted code to the frontend installer:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=frontend \
  OORE_WEB_BACKEND_URL=http://10.0.0.20:8787 \
  OORE_WEB_BACKEND_TRANSPORT_PROTECTED=true \
  OORE_FRONTEND_PAIRING_CODE=fp_replace_with_the_code \
  OORE_NONINTERACTIVE=1 \
  bash
```

Pairing requires a ready backend with Trusted Proxy configured and permits the exchange only from its trusted frontend/proxy CIDRs. Use HTTPS or an encrypted private overlay such as NetBird for that path; the exchange returns the durable backend proof and must not cross an untrusted plaintext network. `OORE_WEB_BACKEND_TRANSPORT_PROTECTED=true` explicitly asserts that this protection is already configured when the backend URL uses remote HTTP. The frontend installer saves the returned proof, persists the protected-transport launcher argument, and creates a separate local upstream proof for the reverse proxy -> `oore-web` hop.

Frontend-only mode:

- Downloads `oore-web` and the prebuilt `web-dist` assets only.
- Supports Linux and macOS release assets.
- Does not install or start `oored`, `oore`, or a Direct macOS runner.
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

### Trusted Proxy through `oore-web`

If the frontend origin is behind an identity-aware proxy, `oore-web` uses two separate proofs:

- backend proof: `OORE_TRUSTED_PROXY_SHARED_SECRET_FILE`, copied from the backend host, lets `oore-web` call `oored` as the trusted proxy hop.
- upstream proof: `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE` lets `oore-web` know the identity header came from your authenticated reverse proxy, not from browser JavaScript.

Your reverse proxy must strip any browser-supplied identity headers, set the configured user email header, and send `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER` (default `x-oore-web-trusted-proxy-secret`) with the upstream proof secret. Without that upstream proof, `oore-web` strips identity headers before proxying API requests.

`OORE_FRONTEND_PAIRING_CODE` is the preferred way to provision those proofs for a Trusted Proxy split deployment. Supplying the two proof-file variables manually is an advanced fallback; their values must differ.

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

For a macOS system service, the updater explains why administrator access is needed and asks `sudo` to authorize the service restart before replacing installed files. Password input is hidden by macOS and is never stored by Oore.

Owners can also check the frontend and backend versions independently from **Settings → Preferences**. When a newer release exists, the page can update a frontend installed as a managed systemd/launchd service and a backend installed as the managed macOS LaunchDaemon. Unmanaged processes remain command-line updates because Oore has no service manager to restart them safely.

Runner inventory reports each runner's installed version. Remote runner updates are not available yet: detached runners currently use the `oore` CLI process and do not have a runner-only package or managed service contract.

## Next step: choose setup path

Before using the hosted UI, ensure your backend is HTTPS-reachable from the browser network path.
`https://ci.oore.build` is UI-only and cannot call `http://127.0.0.1:*` directly.

Choose the path that matches how the browser reaches the backend:

- Local Only backend: run `oore setup` and choose `Local Only`, or run `oore-web --backend-url http://127.0.0.1:8787` and open the local UI.
- HTTPS-reachable backend: open [ci.oore.build](https://ci.oore.build), add the backend URL, and choose `Remote OIDC` or `Remote Trusted Proxy`.
- Split frontend/backend: open your frontend origin, add the instance with **Backend URL** empty so API calls stay on the same origin and flow through `oore-web`.

For local setup UI, open `http://127.0.0.1:4173/setup` (or your configured `OORE_LOCAL_WEB_LISTEN` address).

Continue with [Set Up Your Instance](/getting-started/first-instance). If you plan to use `ci.oore.build`, read [Hosted UI Onboarding](/getting-started/hosted-ui-onboarding) first.

## Installer environment variables

| Variable                                             | Default                                                         | Description                                                                                                                                                           |
| ---------------------------------------------------- | --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `OORE_VERSION`                                       | `latest`                                                        | Release selector (`latest` or tag like `v0.2.0`)                                                                                                                      |
| `OORE_CHANNEL`                                       | `stable`                                                        | Channel selector when `OORE_VERSION=latest`: `stable`, `beta`, or `alpha`                                                                                             |
| `OORE_INSTALL_MODE`                                  | `auto`                                                          | Advanced install mode: `auto`, `all`, `backend`, or `frontend`; use it with `--advanced`; `full` is accepted as a legacy alias for `all`                              |
| `OORE_INSTALL_ROOT`                                  | `~/.oore`                                                       | Installation directory                                                                                                                                                |
| `OORE_GITHUB_REPO`                                   | `oore-ci/oore.build`                                            | GitHub repository used to download release assets                                                                                                                     |
| `OORE_RELEASE_BASE_URL`                              | `https://github.com/<repo>/releases/download`                   | Base URL that contains `<tag>/` release assets                                                                                                                        |
| `OORE_RELEASE_INDEX_BASE_URL`                        | `https://releases.oore.build`                                   | Static release discovery origin; latest manifests live at `/latest/<channel>.json` and channel history at `/<channel>.json`                                           |
| `OORE_RELEASE_MANIFEST_URL`                          | `<release-index>/latest/<channel>.json`                         | Exact latest-channel manifest override used when `OORE_VERSION=latest`                                                                                                |
| `OORE_NONINTERACTIVE`                                | `0`                                                             | Disable prompts when set to `1`                                                                                                                                       |
| `OORE_OPEN_BROWSER`                                  | interactive local install only                                  | Open the local web root after a default macOS install; set `true` to opt in for non-interactive installs                                                              |
| `OORE_DAEMON_LISTEN`                                 | from `OORE_DAEMON_URL`                                          | Daemon listen address for `all` and `backend` installs, for example `127.0.0.1:8787` for same-host reverse proxy or `10.0.0.20:8787` for a private frontend host      |
| `OORE_START_DAEMON`                                  | unset                                                           | Non-interactive daemon startup behavior (`true` or `false`)                                                                                                           |
| `OORE_INSTALL_DAEMON_SERVICE`                        | unset                                                           | Non-interactive launchd service install/start behavior for `all` and `backend` macOS installs (`true` or `false`)                                                     |
| `OORE_PUBLIC_URL`                                    | unset                                                           | Browser-visible HTTPS URL passed to the daemon service as External Access fallback                                                                                    |
| `OORE_CORS_ORIGINS`                                  | `OORE_PUBLIC_URL` when set                                      | Comma-separated allowed browser origins passed to the daemon service                                                                                                  |
| `OORE_DAEMON_URL`                                    | `http://127.0.0.1:8787`                                         | Daemon URL used by backend setup helpers                                                                                                                              |
| `OORE_WEB_BACKEND_URL`                               | `OORE_DAEMON_URL`                                               | Backend URL proxied by `oore-web`, useful for frontend-only hosts                                                                                                     |
| `OORE_WEB_BACKEND_TRANSPORT_PROTECTED`               | `false`                                                         | Assert that an encrypted transport already protects a remote HTTP backend hop; persists `--backend-transport-protected`                                               |
| `OORE_WEB_BROWSER_TRANSPORT_PROTECTED`               | `false`                                                         | Assert that encrypted ingress already protects a non-loopback HTTP web listener; persists `--browser-transport-protected`                                             |
| `OORE_FRONTEND_PAIRING_CODE`                         | unset                                                           | Short-lived, single-use code from `oore frontend invite`; exchanges over the private backend path for the backend proof and generates a separate local upstream proof |
| `OORE_SETUP_OWNER_EMAIL`                             | unset                                                           | Initial owner email to prefill in Trusted Proxy setup                                                                                                                 |
| `OORE_SETUP_PROXY_PRESET`                            | `generic`                                                       | Trusted Proxy setup prefill: `generic`, `warpgate`, or `custom`                                                                                                       |
| `OORE_SETUP_USER_EMAIL_HEADER`                       | unset                                                           | Custom Trusted Proxy email header when `OORE_SETUP_PROXY_PRESET=custom`                                                                                               |
| `OORE_TRUSTED_PROXY_SHARED_SECRET`                   | unset                                                           | Trusted Proxy backend shared secret; installer persists it to a restrictive file when needed                                                                          |
| `OORE_TRUSTED_PROXY_SHARED_SECRET_FILE`              | `~/.oore/trusted-proxy-shared-secret` when generated            | File containing the backend shared secret for `oore setup init` and `oore-web`                                                                                        |
| `OORE_TRUSTED_PROXY_CIDRS`                           | unset                                                           | Comma-separated trusted proxy/frontend peer CIDRs allowed to send identity to the backend                                                                             |
| `OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER`           | preset-derived                                                  | Identity header `oore-web` may forward only after upstream proof                                                                                                      |
| `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET`      | unset                                                           | Auth proxy to `oore-web` proof secret; installer persists it to a restrictive file when needed                                                                        |
| `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE` | `~/.oore/oore-web-upstream-trusted-proxy-secret` when generated | File containing the auth proxy to `oore-web` proof secret                                                                                                             |
| `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SECRET_HEADER`      | `x-oore-web-trusted-proxy-secret`                               | Header your auth proxy sends to prove the identity header is proxy-set                                                                                                |
| `OORE_LOCAL_WEB_MODE`                                | unset                                                           | Non-interactive local web behavior for localhost backends: `off`, `run`, or `login` (launch-at-login)                                                                 |
| `OORE_LOCAL_WEB_LISTEN`                              | `127.0.0.1:4173`                                                | Bind address for `oore-web`                                                                                                                                           |
| `OORE_ENABLE_LINGER`                                 | unset                                                           | Enable systemd lingering for Linux frontend service installs (`true` or `false`)                                                                                      |

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
