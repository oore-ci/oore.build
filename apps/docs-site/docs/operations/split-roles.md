---
status: implemented
description: "Run Oore CI backend and frontend roles on separate hosts."
---

# Split Backend and Frontend Roles

Use this deployment shape when the daemon and build runner should run on one host, while the browser-facing web UI runs on another host.

## Roles

| Role | Install mode | Host support | Installs |
|---|---|---|---|
| All-in-one | `all` | macOS | `oored`, `oore`, embedded runner, `oore-web`, web assets |
| Backend | `backend` | macOS | `oored`, `oore`, embedded runner |
| Frontend | `frontend` | Linux or macOS | `oore-web`, web assets |

`OORE_INSTALL_MODE=auto` prompts for a role on interactive macOS installs. On Linux, it selects `frontend`.

## Network Shape

The frontend host must be able to reach the backend daemon URL. That can be through any private or controlled network path:

- Private LAN
- VPN or mesh network
- Private DNS
- Internal load balancer
- Tunnel or reverse proxy

Keep `oore-web` on loopback when an HTTPS reverse proxy runs on the same frontend host. Bind `oore-web` to another interface only when you intentionally expose it directly.

## Backend Host

Install only the backend role:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=backend \
  bash
```

Non-interactive backend example:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=backend \
  OORE_DAEMON_LISTEN=10.0.0.20:8787 \
  OORE_SETUP_OWNER_EMAIL=owner@example.com \
  OORE_SETUP_PROXY_PRESET=generic \
  OORE_INSTALL_DAEMON_SERVICE=true \
  OORE_NONINTERACTIVE=1 \
  bash
```

Use `127.0.0.1:8787` when a reverse proxy runs on the same backend host. Use a private interface address when a separate frontend host must call the daemon directly.

For frontend-proxy topologies, keep External Access/CORS unset on the backend install unless browsers will call `oored` directly. The installer saves the selected daemon URL for `oore` CLI commands and exits after daemon install/start; first-run setup continues from the frontend URL.

## Frontend Host

For a Trusted Proxy deployment, create a short-lived, single-use pairing code on the ready backend first:

```bash
oore frontend invite
```

Run this as the backend operator on the Mac. The exchange is accepted only from the configured trusted-proxy CIDRs, so ensure the frontend host's private address is allowlisted before continuing. Use HTTPS or an encrypted private overlay for the backend path; pairing transfers the durable backend proof.

Install only the frontend role:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=frontend \
  OORE_WEB_BACKEND_URL=http://10.0.0.20:8787 \
  OORE_LOCAL_WEB_LISTEN=127.0.0.1:4173 \
  OORE_LOCAL_WEB_MODE=login \
  OORE_ENABLE_LINGER=true \
  OORE_FRONTEND_PAIRING_CODE=fp_replace_with_the_code \
  OORE_NONINTERACTIVE=1 \
  bash
```

Put your HTTPS reverse proxy in front of `http://127.0.0.1:4173`. In the web UI, add the instance with **Backend URL** empty so browser API calls stay on the same HTTPS origin and flow through the frontend proxy.

The installer checks the selected listen port before changing service state. If your reverse proxy already owns `4173`, choose another loopback port such as `127.0.0.1:4174` and point the proxy backend at that address.

`OORE_FRONTEND_PAIRING_CODE` exchanges the code with the backend over the private path, writes the returned backend proof into the frontend service's restrictive secret file, and generates a separate local proof for the authenticated reverse proxy -> `oore-web` hop. The pairing code is consumed once and is not saved. `oore-web` proxies `/v1/*`, `/healthz`, and `/readyz` to `OORE_WEB_BACKEND_URL`; browser-supplied identity and proof headers are stripped, and the identity header is forwarded only when the upstream proof matches.

Browsers opening this paired frontend do not need to add the backend manually. When no Oore instance is saved in that browser, the UI recognizes the same-origin `oore-web` proxy and selects it automatically before authentication. Users who already manage multiple instances keep their existing registry unchanged.

Manual `OORE_TRUSTED_PROXY_SHARED_SECRET_FILE` plus `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE` configuration remains available for advanced Trusted Proxy secret-management workflows. Do not set a pairing code and manually reuse either proof value.

During first-run setup, choose `Remote (Trusted Proxy)`, enter the initial owner email, and select a proxy preset. `Generic proxy` uses `x-oore-user-email`, `Warpgate` uses `x-warpgate-username`, and `Custom header` lets you enter the exact header your proxy forwards. The first owner claim must come from that same proxy-authenticated email, avoiding manual database edits.

To pull a newer frontend-only release later:

```bash
oore-web update
```

`oore-web update --check` reports whether the installed channel has a newer release without changing files. Restart the `oore-web` service after an update if you want the running launcher process to pick up binary changes immediately.

Verify both frontend and backend readiness through the installed proxy path:

```bash
oore-web status --url http://127.0.0.1:4173
```

## Provider-Specific Examples

For one concrete private-network setup, see [Mac Studio + NetBird + Warpgate](/operations/mac-studio-netbird-warpgate).
