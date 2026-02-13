---
status: implemented
description: "Install oore.build daemon and CLI binaries on macOS with a single command."
---

# Install oore.build

This page walks you through installing prebuilt backend binaries from the oore release bucket (`dl.oore.build`).

## What you need

- macOS host (V1 backend runtime target)
- `curl`
- Internet access to `dl.oore.build`, `ci.oore.build`, and `docs.oore.build`

## Install (latest release)

```bash
curl -fsSL https://oore.build/install | bash
```

The installer:

- Detects your architecture (`arm64` or `x86_64`)
- Downloads the matching release tarball
- Verifies SHA-256 checksums
- Installs `oored`, `oore`, and `oore-web` under `~/.oore/bin`
- Installs prebuilt local web assets under `~/.oore/web-dist`
- Prompts for optional first-run actions (start daemon, generate setup token, open links)
- For localhost backends, asks whether you plan to expose HTTPS publicly and can start/auto-start local web UI for you

On first daemon start, macOS may show a Keychain permission prompt for `oored`.
This is expected: `oored` stores a local encryption key in your login keychain to protect secrets at rest.
Recommended action on trusted hosts: click **Allow** or **Always Allow**.

## Install a pinned version

```bash
OORE_VERSION=v0.2.0 curl -fsSL https://oore.build/install | bash
```

## Non-interactive mode (automation)

```bash
OORE_NONINTERACTIVE=1 OORE_START_DAEMON=true \
  curl -fsSL https://oore.build/install | bash
```

If `OORE_NONINTERACTIVE=1` and `OORE_START_DAEMON` is not set, daemon startup is skipped.

## Verify installation

```bash
~/.oore/bin/oored version
~/.oore/bin/oore --version
```

## Next step: choose setup path

Before using hosted UI, ensure your backend is HTTPS-reachable from the browser.
`https://ci.oore.build` cannot call `http://127.0.0.1:*` directly.

If your backend is local-only:

- Use CLI setup: `~/.oore/bin/oore setup`, or
- expose it via tunnel first (for example `cloudflared tunnel --url http://127.0.0.1:8787`), or
- run local frontend: `~/.oore/bin/oore-web --backend-url http://127.0.0.1:8787`.
  In the local web UI, add an instance and leave **Backend URL** empty so requests use the built-in proxy.

For hosted setup, open [ci.oore.build](https://ci.oore.build), add your backend URL, and complete setup.  
For local setup UI, open `http://127.0.0.1:4173/setup` (or your configured `OORE_LOCAL_WEB_LISTEN` address).

Continue with [Hosted UI Onboarding](/getting-started/hosted-ui-onboarding).

## Installer environment variables

| Variable | Default | Description |
|---|---|---|
| `OORE_VERSION` | `latest` | Release selector (`latest` or tag like `v0.2.0`) |
| `OORE_INSTALL_ROOT` | `~/.oore` | Installation directory |
| `OORE_RELEASE_BASE_URL` | `https://dl.oore.build/releases` | Base URL that contains `<tag>/` release assets |
| `OORE_RELEASE_MANIFEST_URL` | `https://dl.oore.build/releases/latest.json` | Manifest URL used when `OORE_VERSION=latest` |
| `OORE_NONINTERACTIVE` | `0` | Disable prompts when set to `1` |
| `OORE_START_DAEMON` | unset | Non-interactive daemon startup behavior (`true` or `false`) |
| `OORE_LOCAL_WEB_MODE` | unset | Non-interactive local web behavior for localhost backends: `off`, `run`, or `login` (launch-at-login) |
| `OORE_LOCAL_WEB_LISTEN` | `127.0.0.1:4173` | Bind address for `oore-web` |

## Troubleshooting

### Unsupported architecture

The installer currently supports macOS `arm64` and `x86_64`.

### Checksum mismatch

The installer exits before installing binaries if checksums do not match. Re-run once to rule out transient download issues. If it persists, do not continue and verify release assets in `https://dl.oore.build/releases/<tag>/`.

### Daemon startup failed

Check logs:

```bash
cat ~/.oore/logs/oored.log
```

Then run diagnostics:

```bash
~/.oore/bin/oore doctor
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
OORE_INSTALL_ROOT="$HOME/.oore-user" curl -fsSL https://oore.build/install | bash
```
