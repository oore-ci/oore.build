---
status: placeholder
description: "CLI reference for oore login to authenticate with an Oore CI instance."
---

# oore login

::: warning Not yet implemented
This command is defined in the platform contract but has not been implemented yet. The interface described below reflects the planned behavior.
:::

Authenticate the CLI with a running Oore CI instance.

## Planned synopsis

```bash
oore login [--instance <url>]
```

## Planned behavior

Opens the OIDC login flow in the default browser and stores the session token locally for subsequent CLI commands.

| Flag | Description |
|------|-------------|
| `--instance` | Daemon URL to authenticate against (default: `http://127.0.0.1:8787`) |

## Current workaround

Until `oore login` is implemented, authenticate via the web UI and use the session token from your browser for API calls:

```bash
curl -H "Authorization: Bearer <session_token_from_browser>" \
  http://127.0.0.1:8787/v1/projects
```
