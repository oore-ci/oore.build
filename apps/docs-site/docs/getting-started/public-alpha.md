---
status: implemented
description: 'Public alpha release notes + the fastest paths to first success (and how to avoid common setup blockers).'
---

# Public Alpha (v0.1.x)

Oore CI is in **public alpha**.

- Expect breaking changes (APIs, config formats, and CLI flags) between `v0.1.x` releases.
- We still publish a `stable` channel to mean “the default install/update channel”, not “1.0 production maturity”.

If you’re evaluating Oore CI, this page is the shortest path to your first green build.

Before broader rollout, review: [Known Alpha Limitations](/getting-started/known-limitations).

## Release Channels

Oore CI uses three release channels to balance stability and velocity.

| Channel    | Frequency  | Stability    | Recommended For                          |
| ---------- | ---------- | ------------ | ---------------------------------------- |
| **stable** | ~Weekly    | Highest      | Typical evaluation and production usage. |
| **beta**   | ~Daily     | Moderate     | Previewing upcoming features.            |
| **alpha**  | Per-commit | Experimental | Testing bug fixes or contributing code.  |

### Install/Update Examples

```bash
# Install stable (default)
curl -fsSL https://oore.build/install | bash

# Install alpha
curl -fsSL https://oore.build/install | OORE_CHANNEL=alpha bash

# Update to latest on your current channel
oore update
```

## Auth‑mode decision table

Choosing the right authentication mode depends on where you access your daemon from.

| Mode               | Access      | Auth    | Use Case        |
| ------------------ | ----------- | ------- | --------------- |
| **Local-only**     | `127.0.0.1` | None    | Local eval      |
| **Remote (OIDC)**  | HTTPS       | OIDC    | Team Dashboards |
| **Remote (Proxy)** | Proxy/IAP   | Headers | Private/Ent     |

## The two supported onboarding paths

Choosing the right path depends on your environment and whether your daemon is reachable from the public internet.

| Path            | Use When        | Requirements       | Tradeoffs           |
| --------------- | --------------- | ------------------ | ------------------- |
| **Local-first** | Fast local eval | macOS, loopback    | No remote access    |
| **Hosted UI**   | Teams & Remote  | macOS, HTTPS, OIDC | Needs tunnel + OIDC |

![Oore CI Dashboard screenshot](/demo-dashboard.webp)

### Path A: Local-first (no HTTPS required)

Best when you want to try it on a single Mac first.

1. Install:

```bash
curl -fsSL https://oore.build/install | bash
```

2. Start the daemon:

```bash
oored run
```

3. Complete setup from the CLI:

```bash
oore setup
```

![Oore CI Builds list screenshot](/demo-builds.webp)

Continue with:

- [Install](/getting-started/install)
- [Set Up Your Instance](/getting-started/first-instance)

### Path B: Hosted UI (requires an HTTPS-reachable backend URL)

Best when you want the hosted UI at `https://ci.oore.build` from day one.

**Important constraint**: Browsers block `https` pages (like `ci.oore.build`) from making requests to `http://127.0.0.1` or other `http` origins. You **must** provide an `https://` URL for your backend.

1. Install + start the daemon as above.
2. Make your backend reachable over HTTPS (for example, via a tunnel):

```bash
cloudflared tunnel --url http://127.0.0.1:8787
```

3. Open `https://ci.oore.build`, add your tunnel URL as the backend, and follow the setup wizard.

### Cloudflared Troubleshooting (#43)

If you have trouble connecting your tunnel to the Hosted UI, check these common failure modes:

1. **Tunnel URL has expired**
   - **Symptom**: Cloudflare logo page says "This tunnel is not active."
   - **Fix**: Restart your tunnel command. If using Quick Tunnels, you will get a new URL each time.
   - **Check**: `cloudflared tunnel --url http://127.0.0.1:8787`

2. **Localhost Mismatch (Port 8787)**
   - **Symptom**: "Backend unreachable" from `ci.oore.build` despite tunnel being up.
   - **Fix**: Ensure `oored` is running on the same port your tunnel is pointing to.
   - **Check**: `curl -I http://127.0.0.1:8787/healthz`

3. **Mixed Content / HTTP instead of HTTPS**
   - **Symptom**: Browser console shows "Blocked loading of mixed active content."
   - **Fix**: Ensure your backend URL in `ci.oore.build` starts with `https://`.
   - **Check**: Look for the `trycloudflare.com` URL in your terminal logs.

For a full reset of your Oore CI instance, see the [Clean Reinstall Guide](/getting-started/clean-reinstall).

Continue with:

- [Hosted UI Onboarding](/getting-started/hosted-ui-onboarding)

## Common first-time blockers (and fixes)

### `oore` / `oored` not found after install

The installer adds `~/.oore/bin` to your PATH for future shells.

- Open a new terminal, then try `oore version`, or
- Use the full path once: `~/.oore/bin/oore version`.

### Hosted UI can’t connect to a localhost backend

If your backend URL is `http://127.0.0.1:8787`, the hosted UI will not be able to reach it.

- Use local setup (`oore setup`), or
- expose the backend over HTTPS (tunnel / reverse proxy) and use that URL in hosted UI.

### “Do I need OIDC on day one?”

Remote access defaults to OIDC, but local-first onboarding supports loopback-only login (no local passwords).

If you want a remote-first path without configuring OIDC immediately, see the deployment docs for the `trusted_proxy` option:

- [Deployment](/operations/deployment)

## How to report issues and security findings

- Bugs/UX issues: GitHub issues (use the [Alpha Feedback Playbook](/getting-started/alpha-feedback-playbook))
- Security reports: follow [SECURITY.md](https://github.com/devaryakjha/oore.build/blob/master/SECURITY.md) (private disclosure)
