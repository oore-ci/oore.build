---
status: placeholder
description: "CLI reference for oore status to check daemon and runner health."
---

# oore status

::: warning Not yet implemented
This command is defined in the platform contract but has not been implemented yet. The interface described below reflects the planned behavior.
:::

Show the current state of the Oore CI instance.

## Planned synopsis

```bash
oore status [--instance <url>]
```

## Planned behavior

Queries the daemon and displays instance health, setup state, connected integrations, and runner status.

| Flag | Description |
|------|-------------|
| `--instance` | Daemon URL to query (default: `http://127.0.0.1:8787`) |

## Current workaround

Until `oore status` is implemented, query the public status endpoint directly:

```bash
curl http://127.0.0.1:8787/v1/public/setup-status
```

This returns the current setup state (`uninitialized`, `bootstrap_pending`, `idp_configured`, `owner_created`, or `ready`).
