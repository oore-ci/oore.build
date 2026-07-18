---
status: implemented
description: 'Install Oore CI on one Mac and verify the managed services.'
---

# Install Oore CI

The default installer sets up a complete local instance on one Mac. Use the advanced paths only when the browser-facing frontend and macOS backend must run on different hosts.

## Before you begin

You need:

- a macOS host with `curl`
- internet access to GitHub and the installer endpoint
- the [build prerequisites](/getting-started/prerequisites) required by your projects

## Install on one Mac

```bash
curl -fsSL https://oore.build/install | bash
```

The installer adds `oored`, `oore`, `oore-web`, the Direct runner binary, and the web assets under `~/.oore`. It keeps the daemon and web service on loopback, installs their launch-at-login services, starts them, and opens `http://127.0.0.1:4173`.

Use `--no-open` to keep the installer from opening a browser:

```bash
curl -fsSL https://oore.build/install | bash -s -- --no-open
```

## Verify the installation

Open a new terminal so your shell sees `~/.oore/bin`, then run:

```bash
oored version
oore version
oore-web status --url http://127.0.0.1:4173
```

If a command is not found, run it once with the full path, for example `~/.oore/bin/oore version`.

## Finish runner setup once

After signing in:

1. Register the Direct macOS runner and install its login-session service by following [Run builds with the Direct macOS runner](/guides/runners/external-runner).
2. Enable **Direct macOS runner** in **Settings > Runners**.
3. Approve each repository you intend to build in **Settings > Sources**.

The instance switch and repository approvals default off after a fresh install or upgrade. Once enabled for a trusted repository, normal builds do not require repeated approval.

## Choose a release channel

Stable is the default. Use the matching installer endpoint for prerelease channels:

```bash
# stable
curl -fsSL https://oore.build/install | bash

# beta
curl -fsSL https://beta.oore.pages.dev/install | OORE_CHANNEL=beta bash

# alpha
curl -fsSL https://alpha.oore.pages.dev/install | OORE_CHANNEL=alpha bash
```

See [Public alpha and release channels](/operations/release-channels) before testing a prerelease.

## Update an installed instance

```bash
oore update --check
oore update
```

The managed updater verifies and installs the release, restarts the managed daemon and local web services, and restarts the managed Direct runner service after the backend toolchain is ready. UI-managed backend updates use the same path; a normal managed install does not require SSH or a separate `launchctl kickstart` command. Foreground or otherwise unmanaged processes must be restarted by their operator.

See [Upgrade procedures](/operations/upgrade) for backups, readiness checks, and rollback behavior.

## Other deployment shapes

- Use [Hosted UI onboarding](/getting-started/hosted-ui-onboarding) when `ci.oore.build` will connect to your HTTPS-reachable backend.
- Use [Split backend and frontend roles](/operations/split-roles) when `oore-web` and `oored` run on different hosts.
- Use the [installer reference](/reference/config/installer) for pinned versions, automation, install roles, and environment variables.
- Use [Deployment](/operations/deployment) for reverse proxy, TLS, and production service guidance.

If installation fails, continue to [Troubleshooting](/operations/troubleshooting).
