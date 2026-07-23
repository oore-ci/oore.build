---
status: implemented
description: 'Deploy Oore CI on a Mac Studio with NetBird reachability and Warpgate trusted-proxy auth.'
---

# Mac Studio + NetBird + Warpgate

This is the recommended first company rollout shape for an internal-only Oore CI instance:

- Mac Studio runs `oored` and the separate Direct macOS runner service
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
  -> Direct macOS runner service on Mac Studio
```

In this shape:

- Mac Studio runs `oored` plus a separately enrolled Direct macOS runner service.
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

When the daemon binds a specific NetBird address, it also opens the same port on loopback for the local Direct runner and operator commands. It does not add a wildcard listener; the NetBird address remains the only non-loopback daemon address.

The backend installer enrolls the local Direct runner and installs its separate
boot-time LaunchDaemon as the selected macOS account. If that service ever needs
repair, run `oore runner install-service --managed-local` as the runner account, without `sudo`;
Oore requests administrator access only for launchd setup. An Owner or Admin
trusts repository code by linking it to a project. **Accept new builds** in
Preferences is an operational pause, not a second allowlist. A dedicated
non-admin account is recommended hardening, but Direct mode is not hostile-code
isolation.

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
  OORE_WEB_BACKEND_TRANSPORT_PROTECTED=true \
  OORE_LOCAL_WEB_LISTEN=127.0.0.1:4174 \
  OORE_LOCAL_WEB_MODE=login \
  OORE_ENABLE_LINGER=true \
  OORE_FRONTEND_PAIRING_CODE=fp_replace_with_the_code \
  OORE_NONINTERACTIVE=1 \
  bash
```

`OORE_WEB_BACKEND_TRANSPORT_PROTECTED=true` asserts that NetBird is already protecting the frontend-to-backend HTTP hop; it does not configure or verify NetBird. Set it only after the route and peer policy are active. The installer persists the matching launcher argument in the generated systemd or launchd service.

The installer exchanges the code over NetBird, saves the returned backend proof, and generates a different local HAProxy -> `oore-web` proof. It fails before changing service state if the selected port is occupied. Keep `oore-web` on loopback; HAProxy is the only local process that should call it.

Services generated by an older alpha installer may not contain `--backend-transport-protected`. Before updating one of those services, rerun the current installer with a fresh pairing code and the complete frontend configuration above, or add that argument to the unit after confirming NetBird protects the hop. Then reload the service manager, restart `oore-web`, and run the explicit `:4174` status check below. Managed updates from the web UI preflight the candidate launcher against the active service configuration before replacing files.

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

The loopback readiness request must succeed. After signing in, **Runners** must
show the Direct macOS runner as `online`; backend readiness alone is not
sufficient for a testable build host. Confirm **Accept new builds** is on and the
project's linked source is available before expecting queued builds to be
claimed.

On Ubuntu:

```bash
oore-web status --url http://127.0.0.1:4174
systemctl --user status oore-web.service --no-pager
```

The status command must report both the frontend launcher and Mac backend ready before HAProxy is pointed at the service.

From the browser, open the Warpgate-protected public URL. The authenticated email matching `OORE_SETUP_OWNER_EMAIL` signs in as the existing owner; there is no placeholder `owner@local` account and no database role swap.

Configure External Access `public_url` and `allowed_origins` with that same HTTPS URL after login.

### 6. Configure non-interactive iOS installs

Warpgate normally expects an interactive browser session, while Apple's iOS
installer fetches the manifest and IPA as separate system requests. Create a
Warpgate access ticket bound to a dedicated, least-privilege user and the Oore
target. Give it the shortest practical expiry and use allowance for your QA
workflow. Do not use an owner or staff user's ticket.
Do not invite the dedicated ticket identity into Oore; the short-lived Oore
artifact token, not that identity, authorizes the requested IPA.

In Oore, open **Preferences → Identity settings**. The iOS install ticket field
appears only when the trusted-proxy identity header is
`x-warpgate-username`. Paste the ticket and save. Oore encrypts it at rest and
reports only whether it exists and whether it came from encrypted settings or
the environment.

For service-managed fresh installs, add
`OORE_WARPGATE_TICKET=replace_with_ticket` to the non-interactive backend
installer environment shown earlier. For an existing instance, prefer the UI;
changing a LaunchDaemon environment directly requires reinstalling it with its
complete current listen, user, state-file, and environment arguments.

The Preferences value takes precedence when both sources exist. Remove the
stored value in Preferences to return to the environment fallback. Restart the
daemon after changing its service environment.

For iOS only, Oore appends `warpgate-ticket` to the manifest URL, the IPA URL
inside the manifest, and the final local-storage download redirect. Warpgate
accepts the ticket from that query parameter before the request reaches Oore;
Oore then independently validates its short-lived artifact token. Android,
OIDC, Local Only mode, and generic trusted-proxy deployments never receive the
Warpgate query parameter.

The ticket can appear in browser history and ingress access logs. Keep access
to those logs restricted, rotate the ticket when exposure is suspected, and
retain NetBird as the private network boundary. A separate unauthenticated
`/install/` ingress bypass is not required in this topology.

Verify the public ingress accepts the ticket but Oore still rejects a fake
artifact token:

```bash
curl -i \
  'https://oore.example.com/install/ios/not-a-token/manifest.plist?warpgate-ticket=replace_with_ticket'
```

The response must be Oore JSON with `401 invalid_token`, not a Warpgate login
page. Run this from a device on the same NetBird policy used by QA users.

## Common mistakes

- Binding `oored` to `0.0.0.0`: bind only the Mac private/NetBird address used by the AWS frontend.
- Reusing HAProxy's listener port for `oore-web`: the launcher needs its own loopback port.
- Reusing one secret for both proxy hops: compromise of one boundary would then compromise both.
- Sending `X-Oore-Trusted-Proxy-Secret` from HAProxy: `oore-web` strips it and injects its own backend proof.
- Using HTTP for the browser-visible origin: External Access expects a non-loopback HTTPS origin.
- Serving the UI from one origin and API from another without adding the UI origin to `allowed_origins`.
- Forgetting to forward `X-Warpgate-Username` to `/v1/*`.
- Passing a username instead of an email in the trusted-proxy header.
- Creating the install ticket for a privileged Warpgate user instead of a dedicated target-only identity.
- Configuring a ticket while Oore uses OIDC or a different identity header; the integration intentionally remains inactive.

## When to use OIDC instead

Choose OIDC instead of trusted proxy when:

- Warpgate is not the identity boundary
- users access the instance directly rather than through the proxy
- you want the browser to complete the standard OIDC redirect flow against your IdP
