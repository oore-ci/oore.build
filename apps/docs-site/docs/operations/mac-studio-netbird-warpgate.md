---
status: implemented
description: "Deploy Oore CI on a Mac Studio with NetBird reachability and Warpgate trusted-proxy auth."
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

The installer creates the backend proof at:

```text
~/.oore/trusted-proxy-shared-secret
```

Transfer that file to the Ubuntu service account through your approved secret-delivery path. Do not paste it into shell history or store it in the repository.

### 2. Prepare the Ubuntu proof files

Store the transferred backend proof at a user-readable, mode-`0600` path such as:

```text
~/.config/oore/backend-proof
```

Create a different random frontend proof for HAProxy and `oore-web`, also mode `0600`, at:

```text
~/.config/oore/frontend-proof
```

Configure HAProxy through its protected runtime configuration to send that frontend proof in `X-Oore-Web-Trusted-Proxy-Secret`. The `oore-web` service reads the same value from the frontend proof file.

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
  OORE_TRUSTED_PROXY_SHARED_SECRET_FILE="$HOME/.config/oore/backend-proof" \
  OORE_WEB_TRUSTED_PROXY_USER_EMAIL_HEADER=x-warpgate-username \
  OORE_WEB_UPSTREAM_TRUSTED_PROXY_SHARED_SECRET_FILE="$HOME/.config/oore/frontend-proof" \
  OORE_NONINTERACTIVE=1 \
  bash
```

The installer now fails before changing service state if the selected port is occupied. Keep `oore-web` on loopback; HAProxy is the only local process that should call it.

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

`OORE_WEB_FRONTEND_PROOF` represents the protected HAProxy runtime value matching `~/.config/oore/frontend-proof`; wire it using your existing HAProxy service-secret mechanism. Warpgate must overwrite, not merely pass through, `X-Warpgate-Username`. Network policy must ensure clients cannot reach this HAProxy listener without Warpgate.

### 5. Verify before opening access

On Ubuntu:

```bash
oore-web status --url http://127.0.0.1:4174
systemctl --user status oore-web.service --no-pager
```

The status command must report both the frontend launcher and Mac backend ready before HAProxy is pointed at the service.

From the browser, open the Warpgate-protected public URL. The authenticated email matching `OORE_SETUP_OWNER_EMAIL` signs in as the existing owner; there is no placeholder `owner@local` account and no database role swap.

Configure External Access `public_url` and `allowed_origins` with that same HTTPS URL after login.

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
