---
status: implemented
description: 'CLI reference for oore config key/value management.'
---

# oore config

Manage local CLI defaults used by commands like `oore login` and `oore status`.

## Synopsis

```bash
oore config set <key> <value>
oore config get <key>
```

## Supported keys (alpha tranche)

| Key             | Meaning                                              |
| --------------- | ---------------------------------------------------- |
| `daemon_url`    | Default daemon URL                                   |
| `session_token` | Default session token for authenticated CLI commands |

Unsupported keys fail with exit code `2`.

## Storage

- Default path: `~/.oore/config.json`
- Override path: `OORE_CONFIG_FILE`

## Examples

```bash
oore config set daemon_url http://127.0.0.1:8787
oore config set session_token <token>

oore config get daemon_url
oore config get session_token
```
