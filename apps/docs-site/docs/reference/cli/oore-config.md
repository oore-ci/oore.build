---
status: placeholder
description: "CLI reference for oore config to manage daemon configuration."
---

# oore config

::: warning Not yet implemented
This command is defined in the platform contract but has not been implemented yet. The interface described below reflects the planned behavior.
:::

Get and set CLI configuration values.

## Planned synopsis

```bash
oore config set <key> <value>
oore config get <key>
```

## Planned subcommands

| Subcommand | Description |
|------------|-------------|
| `set` | Store a configuration value |
| `get` | Retrieve a configuration value |

## Planned configuration keys

| Key | Description |
|-----|-------------|
| `instance.url` | Default daemon URL |
| `instance.name` | Human-readable instance label |

## Current workaround

Until `oore config` is implemented, configure the daemon URL via environment variables:

```bash
export OORED_LISTEN_ADDR=127.0.0.1:8787
```

See [Environment Variables](/reference/config/environment-variables) for all available variables.
