---
status: implemented
description: "Public alpha release notes + the fastest paths to first success (and how to avoid common setup blockers)."
---

# Public Alpha (v0.1.x)

oore.build is in **public alpha**.

- Expect breaking changes (APIs, config formats, and CLI flags) between `v0.1.x` releases.
- We still publish a `stable` channel to mean “the default install/update channel”, not “1.0 production maturity”.

If you’re evaluating oore.build, this page is the shortest path to your first green build.

## The two supported onboarding paths

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

Continue with:
- [Install](/getting-started/install)
- [Set Up Your Instance](/getting-started/first-instance)

### Path B: Hosted UI (requires an HTTPS-reachable backend URL)

Best when you want the hosted UI at `https://ci.oore.build` from day one.

Important constraint: a browser page loaded from `https://ci.oore.build` cannot call `http://127.0.0.1:*`.

1. Install + start the daemon as above.
2. Make your backend reachable over HTTPS (for example, via a tunnel):

```bash
cloudflared tunnel --url http://127.0.0.1:8787
```

3. Open `https://ci.oore.build`, add your tunnel URL as the backend, and follow the setup wizard.

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

- Bugs/UX issues: GitHub issues
- Security reports: follow [SECURITY.md](https://github.com/devaryakjha/oore.build/blob/master/SECURITY.md) (private disclosure)

