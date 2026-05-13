---
status: implemented
description: "CLI reference for user administration commands."
---

# oore users

Administrative user commands.

## Transfer owner

```bash
oore users transfer-owner --email owner@example.com
```

Transfers the singleton `owner` role to an existing active user. The previous owner is demoted to `admin`, the setup owner record is updated, and sessions for both users are revoked so the new role is picked up on the next sign-in.

### Flags

| Flag | Env var | Required | Description |
|---|---|---:|---|
| `--daemon-url <url>` | `OORE_DAEMON_URL` | No | Backend daemon URL. Defaults to CLI config or `http://127.0.0.1:8787`. |
| `--token <token>` | `OORE_SESSION_TOKEN` | No | Current owner session token. Defaults to stored CLI config. |
| `--email <email>` |  | Yes | Active user who should become owner. |
| `--json` |  | No | Print the transfer response as JSON. |

The target user must already exist and have `active` status. Invite the user and have them sign in once before transferring ownership.
