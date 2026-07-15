---
status: implemented
description: 'Deploy Oore CI on a Mac Studio with NetBird reachability and Warpgate trusted-proxy auth.'
---

# Mac Studio + NetBird + Warpgate

This is the recommended first company rollout shape for an internal-only Oore CI instance:

- Mac Studio runs `oored` and the embedded runner
- NetBird provides private network reachability
- Ubuntu runs the browser-facing `oore-web` behind Warpgate and HAProxy

Use this when many users need the UI but only a small operator group may access the Mac Studio. The daemon has no browser-facing route: the AWS host is its only permitted network peer.

## Architecture

```text
Browser
  -> Warpgate on Ubuntu (auth)
  -> HAProxy on Ubuntu
  -> oore-web on a separate loopback port
  -> NetBird
  -> oored on Mac Studio NetBird address:8787
  -> embedded runner on Mac Studio
```

In this shape:

- Mac Studio runs `oored` and the embedded runner.
- Ubuntu runs only `oore-web` plus the static frontend assets.
- Warpgate overwrites `X-Warpgate-Username` with the authenticated email before forwarding the request to HAProxy.
- HAProxy is reachable only from Warpgate, forwards that identity, and adds the frontend proof header expected by `oore-web`.
- `oore-web` validates the frontend proof, strips browser-controlled identity/proof headers, then injects a separate backend proof when proxying `/v1/*` to the Mac daemon.
- Users add the instance with an empty **Backend URL** so browser requests stay on the HTTPS frontend origin.

The two proxy proofs are intentionally different:

- **Frontend proof:** HAProxy -> `oore-web`. It proves the identity header came through the authenticated frontend path.
- **Backend proof:** `oore-web` -> `oored`. It proves the AWS frontend is the trusted backend peer.

Do not send the backend proof from the browser-facing proxy. Do not configure both hops with one shared value.

## Fresh split deployment

Choose an `oore-web` loopback port that is not already owned by HAProxy. The examples use `4174` because HAProxy commonly owns `4173`; keep the existing HAProxy listener unchanged.

### 1. Install and initialize the Mac backend

Install the backend on the Mac Studio's NetBird address. Backend-owned initialization creates the real owner immediately and avoids the browser bootstrap-token flow:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | OORE_CHANNEL=alpha OORE_INSTALL_MODE=backend bash
```

Non-interactive Mac Studio equivalent:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=backend \
  OORE_DAEMON_LISTEN=100.64.10.20:8787 \
  OORE_SETUP_OWNER_EMAIL=owner@example.com \
  OORE_SETUP_PROXY_PRESET=warpgate \
  OORE_TRUSTED_PROXY_CIDRS=100.64.10.30/32 \
  OORE_INSTALL_DAEMON_SERVICE=true \
  OORE_NONINTERACTIVE=1 \
  bash
```

Replace `100.64.10.20` with the Mac NetBird address, `100.64.10.30/32` with the AWS frontend's NetBird address, and use the real initial owner email. Keep `OORE_PUBLIC_URL` and browser CORS unset because browsers reach the API through same-origin `oore-web`.

Backend-only macOS installs use a system LaunchDaemon running as the installing account. The installer asks for `sudo` so the daemon starts at boot without a GUI login session.

When the daemon binds a specific NetBird address, it also opens the same port on loopback for the embedded runner and local operator commands. It does not add a wildcard listener; the NetBird address remains the only non-loopback daemon address.

### 2. Create a frontend pairing code

On the Mac, after backend setup is ready, create a short-lived single-use code:

```bash
oore frontend invite
```

The exchange only accepts the Ubuntu host's NetBird address (`100.64.10.30/32` in this example), which is already configured through `OORE_TRUSTED_PROXY_CIDRS`. Treat the code as a secret until the installer consumes it; create a new code if it expires or is used.

### 3. Install the Ubuntu frontend

Install the frontend-only bundle on an unused loopback port:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | OORE_CHANNEL=alpha bash
```

Non-interactive Ubuntu equivalent:

```bash
curl -fsSL https://alpha.oore.pages.dev/install | \
  OORE_CHANNEL=alpha \
  OORE_INSTALL_MODE=frontend \
  OORE_WEB_BACKEND_URL=http://100.64.10.20:8787 \
  OORE_LOCAL_WEB_LISTEN=127.0.0.1:4174 \
  OORE_LOCAL_WEB_MODE=login \
  OORE_ENABLE_LINGER=true \
  OORE_FRONTEND_PAIRING_CODE=fp_replace_with_the_code \
  OORE_NONINTERACTIVE=1 \
  bash
```

The installer exchanges the code over NetBird, saves the returned backend proof, and generates a different local HAProxy -> `oore-web` proof. It fails before changing service state if the selected port is occupied. Keep `oore-web` on loopback; HAProxy is the only local process that should call it.

For the Linux user service to survive logout and reboot, the installer can run this when `OORE_ENABLE_LINGER=true`:

```bash
sudo loginctl enable-linger "$USER"
```

### 4. Point HAProxy at `oore-web`

The existing HAProxy frontend keeps its current listener. Its Oore backend must target the separate loopback port:

```text
backend oore_web
    mode http
    http-request del-header X-Oore-Trusted-Proxy-Secret
    http-request set-header X-Oore-Web-Trusted-Proxy-Secret "${OORE_WEB_FRONTEND_PROOF}"
    server oore_web 127.0.0.1:4174 check
```

`OORE_WEB_FRONTEND_PROOF` represents the protected HAProxy runtime value matching the separate local proof generated by the frontend installer; wire it using your existing HAProxy service-secret mechanism. Warpgate must overwrite, not merely pass through, `X-Warpgate-Username`. Network policy must ensure clients cannot reach this HAProxy listener without Warpgate.

Manual backend-proof transfer and a manually managed frontend proof remain an advanced fallback. Configure `OORE_TRUSTED_PROXY_SHARED_SECRET_FILE` and `OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE` with different mode-`0600` files only when you intentionally manage that distribution yourself.

### 5. Verify before opening access

On the Mac:

```bash
curl -fsS http://127.0.0.1:8787/readyz
```

The loopback readiness request must succeed. After signing in, **Runners** must show the embedded runner as `online`; backend readiness alone is not sufficient for a testable build host.

On Ubuntu:

```bash
oore-web status --url http://127.0.0.1:4174
systemctl --user status oore-web.service --no-pager
```

The status command must report both the frontend launcher and Mac backend ready before HAProxy is pointed at the service.

From the browser, open the Warpgate-protected public URL. The authenticated email matching `OORE_SETUP_OWNER_EMAIL` signs in as the existing owner; there is no placeholder `owner@local` account and no database role swap.

Configure External Access `public_url` and `allowed_origins` with that same HTTPS URL after login.

### 6. Route token-only installs around interactive auth

Warpgate requires an interactive session, while the iOS installer fetches its manifest and IPA outside Safari. Keep the Oore UI behind Warpgate, but configure the public TLS ingress in front of Warpgate to send only `GET` and `HEAD` requests under `/install/` directly to the existing HAProxy → `oore-web` path.

Reject every other unauthenticated method and path at the edge. Every `/install/` request is authorized by a short-lived, artifact-scoped token; these endpoints do not accept a user identity header as authorization. Keep **Public URL** set to the normal Warpgate-protected Oore origin. No additional hostname or certificate is required.

If Warpgate owns the public listener, add the path split at a TLS reverse proxy before Warpgate. An Oore route alone cannot bypass authentication that has already redirected the request.

Verify an invalid token reaches Oore rather than an interactive login page:

```bash
curl -i https://oore.example.com/install/ios/not-a-token/manifest.plist
```

The response must be Oore JSON with `401 invalid_token`, not a `3xx` redirect.

## Common mistakes

- Binding `oored` to `0.0.0.0`: bind only the Mac private/NetBird address used by the AWS frontend.
- Reusing HAProxy's listener port for `oore-web`: the launcher needs its own loopback port.
- Reusing one secret for both proxy hops: compromise of one boundary would then compromise both.
- Sending `X-Oore-Trusted-Proxy-Secret` from HAProxy: `oore-web` strips it and injects its own backend proof.
- Using HTTP for the browser-visible origin: External Access expects a non-loopback HTTPS origin.
- Serving the UI from one origin and API from another without adding the UI origin to `allowed_origins`.
- Forgetting to forward `X-Warpgate-Username` to `/v1/*`.
- Passing a username instead of an email in the trusted-proxy header.

## When to use OIDC instead

Choose OIDC instead of trusted proxy when:

- Warpgate is not the identity boundary
- users access the instance directly rather than through the proxy
- you want the browser to complete the standard OIDC redirect flow against your IdP
