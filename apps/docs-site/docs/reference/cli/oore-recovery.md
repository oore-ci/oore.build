---
status: implemented
description: 'Mint a short-lived, single-use browser recovery link on the daemon host.'
---

# oore recovery

Create a browser recovery link for a Ready Remote instance without adding a password or weakening normal OIDC or Trusted Proxy sign-in.

## Synopsis

```bash
oore recovery [--email <email>] [--web-url <url>] [--ttl <duration>] [--state-file <path>] [--json]
```

## Flags

| Flag           | Env var                 | Default                   | Description                                                 |
| -------------- | ----------------------- | ------------------------- | ----------------------------------------------------------- |
| `--email`      | —                       | Single active account     | Account to bind; required when multiple accounts are active |
| `--web-url`    | `OORE_WEB_URL`          | `http://127.0.0.1:4173`   | Web UI base URL used for the recovery link                  |
| `--ttl`        | —                       | `5m`                      | Capability lifetime; must be between 1 second and 5 minutes |
| `--state-file` | `OORE_SETUP_STATE_FILE` | Platform database default | Database path used to derive the management socket path     |
| `--json`       | —                       | `false`                   | Print machine-readable output                               |

## Behavior

`oore recovery` connects only to `<database-directory>/run/oored-management.sock`. The daemon starts that Unix-domain socket with a `0700` parent directory and `0600` socket, both owned by its effective user. The CLI rejects symlinks, wrong ownership, or unexpected modes before connecting.

The daemon binds the capability to the selected active account, keeps only its SHA-256 hash in memory, limits active capabilities, and expires it within five minutes. The link carries the raw capability only in the URL fragment. The web client removes that fragment from browser history before sending the capability once in the local-login POST body.

The capability is consumed atomically. A replay, expiry, malformed value, unknown value, or account mismatch fails. Reconfiguration and daemon shutdown clear all outstanding capabilities.

## Examples

```bash
# One active account, local web UI
oore recovery

# Select an account and send the browser to the deployed UI
oore recovery --email owner@example.com --web-url https://ci.example.com

# Shorten the validity window
oore recovery --ttl 60s
```

Treat the printed URL as a secret until it is used or expires. Do not paste it into tickets, chat, shell arguments, or logs. If the daemon runs under another macOS account, run the command as that account so filesystem ownership checks continue to fail closed.
