---
status: implemented
description: 'CLI reference for oore status (setup summary + authenticated queue/build/runner details).'
---

# oore status

Show instance state.  
With a valid session token, also includes queue/build/runner operational details.

## Synopsis

```bash
oore status [--daemon-url <url>] [--token <session_token>] [--json]
```

## Flags

| Flag           | Env var              | Description                                                         |
| -------------- | -------------------- | ------------------------------------------------------------------- |
| `--daemon-url` | `OORE_DAEMON_URL`    | Daemon URL (defaults to config file, then `http://127.0.0.1:8787`)  |
| `--token`      | `OORE_SESSION_TOKEN` | Session token for authenticated details (falls back to config file) |
| `--json`       | —                    | Emit machine-readable summary                                       |

## Output modes

### Unauthenticated (no token)

Always includes setup summary:

- daemon URL
- instance id
- setup state
- runtime mode
- setup in-progress/complete

### Authenticated (valid token)

Adds:

- queue depth (`status=queued`)
- active build count (`queued + running`)
- recent builds (latest 5)
- runner inventory

## Examples

```bash
# setup-only summary
oore status

# authenticated details
oore status --token <session_token>

# JSON output for automation
oore status --json
```
