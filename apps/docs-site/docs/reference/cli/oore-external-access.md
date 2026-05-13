---
status: implemented
description: "CLI reference for configuring external access and trusted-proxy auth."
---

# oore external-access

Configure browser-facing External Access settings from the operator CLI.

## Enable trusted-proxy mode

```bash
oore external-access enable-trusted-proxy \
  --public-url https://oore.example.com \
  --proxy-cidr 100.107.126.166/32
```

This command applies the settings normally changed by separate API calls:

- External Access public URL
- allowed browser origins
- trusted proxy identity header and CIDRs
- instance preferences: `runtime_mode=remote`, `remote_auth_mode=trusted_proxy`, `key_storage_mode=file`

### Flags

| Flag | Env var | Required | Description |
|---|---|---:|---|
| `--daemon-url <url>` | `OORE_DAEMON_URL` | No | Backend daemon URL. Defaults to CLI config or `http://127.0.0.1:8787`. |
| `--token <token>` | `OORE_SESSION_TOKEN` | No | Owner/admin session token. Defaults to stored CLI config. |
| `--public-url <url>` |  | Yes | HTTPS URL users open in the browser. |
| `--allowed-origin <url>` |  | No | Allowed browser origin. Repeatable. Defaults to `--public-url`. |
| `--proxy-cidr <cidr>` |  | Yes | Source CIDR for the reverse proxy as seen by `oored`. Repeatable. |
| `--user-email-header <header>` |  | No | Header containing the authenticated email. Defaults to `x-warpgate-username`. |
| `--shared-secret <secret>` |  | No | Optional proxy shared secret. |
| `--json` |  | No | Print raw API responses as JSON. |

### Warpgate example

```bash
oore login --token "$TOKEN"

oore external-access enable-trusted-proxy \
  --public-url https://oore.zerodha.io \
  --proxy-cidr 100.107.126.166/32 \
  --user-email-header x-warpgate-username
```

After changing auth mode, existing sessions may be revoked. Sign in again through the Warpgate-protected URL.
