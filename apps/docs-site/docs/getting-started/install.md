---
status: implemented
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
- Installs `oored` and `oore` under `~/.oore/bin`
- Prompts for optional first-run actions (start daemon, generate setup token, open links)

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

## Next step: connect from hosted UI

After installation, open [ci.oore.build](https://ci.oore.build), add your backend instance URL, and complete setup.

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
