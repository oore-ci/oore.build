---
status: implemented
description: "Deploy Oore CI on a Mac Studio with NetBird reachability and Warpgate trusted-proxy auth."
---

# Mac Studio + NetBird + Warpgate

This is the recommended first company rollout shape for an internal-only Oore CI instance:

- Mac Studio runs `oored` and the embedded runner
- NetBird provides private network reachability
- Warpgate protects access and forwards user identity
- A same-host reverse proxy serves the web UI and proxies API requests to `oored`

Use this when you want the UI visible only while connected to your VPN, without exposing the daemon directly on the LAN or internet.

## Target architecture

```text
Browser on VPN
  -> Warpgate (auth)
  -> Caddy/nginx on Mac Studio (HTTPS UI + API proxy)
  -> oored on 127.0.0.1:8787
  -> embedded runner on same Mac Studio
```

Keep `oored` bound to loopback. Expose HTTPS from the reverse proxy, not from the daemon itself.

For the split frontend variant below, bind `oored` to the Mac Studio's NetBird address instead of loopback, and firewall it so only the frontend host can reach the daemon.

## Split frontend architecture

Use this variant when the Mac Studio should stay private on NetBird and a small Ubuntu host should own HTTPS, Warpgate, and the static web UI:

```text
Browser
  -> Warpgate on Ubuntu (auth)
  -> Caddy/nginx on Ubuntu (HTTPS)
  -> oore-web on 127.0.0.1:4173
  -> NetBird
  -> oored on Mac Studio NetBird address:8787
  -> embedded runner on Mac Studio
```

In this shape:

- Mac Studio runs `oored` and the embedded runner.
- Ubuntu runs only `oore-web` plus the static frontend assets.
- Warpgate forwards the authenticated identity header to the Ubuntu reverse proxy.
- The Ubuntu reverse proxy forwards that header to `oore-web`.
- `oore-web` forwards `/v1/*` requests, including Warpgate identity headers and cookies, to the Mac daemon over NetBird.
- Users add the instance with an empty **Backend URL** so browser requests stay on the HTTPS frontend origin.

On the Mac Studio, start the daemon on the NetBird address:

```bash
export OORED_LISTEN_ADDR=100.64.10.20:8787
export OORE_CORS_ORIGINS=https://ci.builds.example.corp
export RUST_LOG=info

oored run --listen 100.64.10.20:8787
```

Install the frontend-only bundle on Ubuntu:

```bash
curl -fsSL https://oore.build/install | \
  OORE_INSTALL_MODE=frontend \
  OORE_WEB_BACKEND_URL=http://100.64.10.20:8787 \
  OORE_LOCAL_WEB_LISTEN=127.0.0.1:4173 \
  OORE_LOCAL_WEB_MODE=login \
  OORE_NONINTERACTIVE=1 \
  bash
```

Replace `100.64.10.20` with the Mac Studio NetBird IP or DNS name. Keep `OORE_LOCAL_WEB_LISTEN` on loopback unless you have a reason to expose the launcher directly.

For the Linux user service to survive logout and reboot:

```bash
sudo loginctl enable-linger "$USER"
```

Example Caddyfile on Ubuntu:

```caddy
ci.builds.example.corp {
    reverse_proxy 127.0.0.1:4173 {
        header_up Host {host}
        header_up X-Forwarded-Proto https
        header_up X-Forwarded-For {remote_host}
        header_up X-Warpgate-Username {header.X-Warpgate-Username}
        header_up X-Oore-Trusted-Proxy-Secret {env.OORE_TRUSTED_PROXY_SHARED_SECRET}
    }
}
```

Configure the Mac daemon's trusted proxy CIDRs to allow the Ubuntu NetBird address or subnet, because the backend sees the request as coming from the frontend host over NetBird rather than loopback.

## 1. Build the binaries and web UI

```bash
bun install
bun run build:web
cargo build --release -p oored
cargo build --release -p oore
```

Artifacts:

- daemon: `target/release/oored`
- operator CLI: `target/release/oore`
- web UI: `apps/web/dist`

## 2. Start the daemon on loopback

Choose the VPN-visible HTTPS origin you want users to open, for example:

- `https://ci.macstudio.internal`
- `https://ci.builds.example.corp`

Then start `oored` on loopback:

```bash
export OORED_LISTEN_ADDR=127.0.0.1:8787
export OORE_CORS_ORIGINS=https://ci.macstudio.internal
export RUST_LOG=info

./target/release/oored run --listen 127.0.0.1:8787
```

Notes:

- The embedded runner starts automatically in the default daemon mode, so a low-risk first app can build on the same Mac Studio.
- Keep the daemon private. Do not bind it to `0.0.0.0` for this topology.

## 3. Serve the UI and API behind internal HTTPS

Example Caddyfile for the same Mac Studio:

```caddy
ci.macstudio.internal {
    root * /absolute/path/to/oore.build/apps/web/dist
    file_server

    handle /v1/* {
        reverse_proxy 127.0.0.1:8787 {
            header_up Host {host}
            header_up X-Forwarded-Proto https
            header_up X-Forwarded-For {remote_host}
            header_up X-Warpgate-Username {header.X-Warpgate-Username}
            header_up X-Oore-Trusted-Proxy-Secret {env.OORE_TRUSTED_PROXY_SHARED_SECRET}
        }
    }

    handle /healthz {
        reverse_proxy 127.0.0.1:8787
    }

    handle /metrics {
        reverse_proxy 127.0.0.1:8787
    }
}
```

Important details:

- Serve `apps/web/dist` from the same HTTPS origin users open in the browser.
- Preserve the Warpgate identity header when proxying `/v1/*`.
- If you configure a trusted-proxy shared secret in Oore, forward the same value as `X-Oore-Trusted-Proxy-Secret` from the reverse proxy.
- If Warpgate and the reverse proxy both run on the Mac Studio, `oored` will see the proxy peer as loopback, so `trusted_proxy_cidrs` can stay empty.
- If `oored` sees a non-loopback proxy peer, add that proxy source CIDR during trusted-proxy setup.

## 4. Put Warpgate in front of the HTTPS origin

Configure Warpgate to:

- require user authentication before access
- forward the authenticated email in `X-Warpgate-Username`
- proxy traffic to the HTTPS origin served by Caddy/nginx

Oore expects the forwarded identity to be an email address by default.

## 5. Complete setup from the VPN-visible UI

Generate a bootstrap token on the Mac Studio:

```bash
./target/release/oore setup token --ttl 15m
```

Then, from a browser connected through NetBird, open:

```text
https://ci.macstudio.internal/setup
```

In setup:

1. Verify the bootstrap token.
2. Choose `Remote (Trusted Proxy / Warpgate)`.
3. Use `x-warpgate-username` as the user email header.
4. Set a shared secret and configure the reverse proxy to send it as `X-Oore-Trusted-Proxy-Secret`.
5. Leave trusted proxy CIDRs empty if the reverse proxy talks to `oored` over loopback.
6. Claim the owner account from the Warpgate-authenticated request.
7. Finalize setup.

## 6. Set External Access network settings after setup

After the owner account is created, open **Settings -> Preferences -> External Access** and set:

- `public_url`: `https://ci.macstudio.internal`
- `allowed_origins`: include `https://ci.macstudio.internal`

This keeps runtime auth, callback validation, and artifact links aligned with the VPN-visible HTTPS origin.

## 7. Verify from a VPN-connected client

```bash
curl -I https://ci.macstudio.internal
curl https://ci.macstudio.internal/healthz
curl https://ci.macstudio.internal/v1/public/setup-status
```

Expected outcomes:

- the UI HTML loads over HTTPS
- `/healthz` returns `200 OK`
- `/v1/public/setup-status` returns JSON
- the login page signs in through Warpgate without loopback-only errors

## Common mistakes

- Binding `oored` directly to `0.0.0.0`: this bypasses the intended trust boundary.
- Using HTTP for the browser-visible origin: External Access expects a non-loopback HTTPS origin.
- Serving the UI from one origin and API from another without adding the UI origin to `allowed_origins`.
- Forgetting to forward `X-Warpgate-Username` to `/v1/*`.
- Passing a username instead of an email in the trusted-proxy header.

## When to use OIDC instead

Choose OIDC instead of trusted proxy when:

- Warpgate is not the identity boundary
- users access the instance directly rather than through the proxy
- you want the browser to complete the standard OIDC redirect flow against your IdP
