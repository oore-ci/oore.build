---
status: implemented
description: 'CLI reference for oore login in alpha mode (token import + local login).'
---

# oore login

Authenticate the CLI and persist credentials for later commands such as `oore status`.

## Synopsis

```bash
# Local-mode login (loopback-only endpoint rules are enforced by daemon)
oore login [--daemon-url <url>] [--email <email>]

# Import and validate an existing session token
oore login --token <session_token> [--daemon-url <url>]

# JSON output
oore login --json ...
```

## Flags

| Flag           | Env var           | Description                                                                |
| -------------- | ----------------- | -------------------------------------------------------------------------- |
| `--daemon-url` | `OORE_DAEMON_URL` | Daemon URL (defaults to config file, then `http://127.0.0.1:8787`)         |
| `--token`      | —                 | Import an existing session token and validate via `/v1/users/me`           |
| `--email`      | —                 | Optional local-login email override (`owner@local` by default server-side) |
| `--json`       | —                 | Print machine-readable output                                              |

## Behavior

- If `--token` is provided, `oore` validates the token against `/v1/users/me`.
- If `--token` is not provided, `oore` uses local login (`POST /v1/auth/local/login`).
- On success, `oore` stores:
  - `daemon_url`
  - `session_token`
    in `~/.oore/config.json` (or `OORE_CONFIG_FILE` override).

## Examples

```bash
oore login --daemon-url http://127.0.0.1:8787
oore login --token <session_token_from_web_ui>
oore login --json
```

## Notes

- Full terminal OIDC browser flow is intentionally deferred in this alpha tranche.
- Local login succeeds/fails based on daemon loopback/auth mode policy.
