---
status: implemented
description: "Connect your self-hosted Oore CI backend to the hosted UI at ci.oore.build."
---

# Hosted UI Onboarding

Use this guide after installing backend binaries to complete setup from `https://ci.oore.build`.

::: warning Hosted UI reachability rule
`https://ci.oore.build` can only connect to backends that are reachable over **HTTPS** from the public internet (or your browser network path).
It cannot call `http://127.0.0.1:*` or other local-only HTTP addresses.
:::

## Preflight checks

Before opening the Hosted UI, verify your backend is reachable over HTTPS from your local machine (where you run the browser). Replace `YOUR_URL` with your actual backend URL.

1. **Verify HTTPS connection**:
   ```bash
   # Should return HTTP 200/401/403, not a timeout or DNS error
   curl -I https://YOUR_URL/healthz
   ```

2. **Verify public status reachability**:
   ```bash
   # Should return JSON with "ready": false (if not set up)
   curl https://YOUR_URL/v1/public/setup-status
   ```

If these commands fail, check your tunnel/reverse proxy configuration and ensure the Oore daemon is running. See [Troubleshooting](/operations/troubleshooting) if you hit DNS or SSL errors.

## 1. Start the daemon

```bash
oored run --listen 127.0.0.1:8787
```

Keep the daemon on loopback. For remote browser access, expose it through an HTTPS reverse proxy instead of binding `oored` directly to a public interface.

## 2. Confirm backend health

```bash
curl http://127.0.0.1:8787/healthz
curl http://127.0.0.1:8787/v1/public/setup-status
```

## 3. Generate a setup token

```bash
oore setup token --ttl 15m
```

Keep this token ready for the setup wizard.

## 4. Choose your setup path

### Option A: Backend already reachable over HTTPS (recommended for hosted UI)

1. Open [ci.oore.build](https://ci.oore.build).
2. Use **Add Instance**.
3. Enter your backend URL (for example `https://ci.your-company.internal`).
4. Continue to `/setup` and paste the bootstrap token.

This can be a VPN-only HTTPS origin. It does not need to be public on the internet, but it must be reachable from the browser network path and use HTTPS.

### Option B: Backend is local-only (no public HTTPS endpoint)

Choose one:

1. **CLI-only setup**
   - Run:
     ```bash
     oore setup
     ```
2. **Temporary tunnel**
   - Expose your backend via Cloudflare Tunnel:
     ```bash
     cloudflared tunnel --url http://127.0.0.1:8787
     ```
   - Add the assigned `https://*.trycloudflare.com` URL in `ci.oore.build`.
3. **Self-host/local frontend**
   - Run the bundled local web UI and connect directly to your local backend:
     ```bash
     oore-web --backend-url http://127.0.0.1:8787
     ```
   - Then open `http://127.0.0.1:4173`.
   - Add an instance and leave **Backend URL** empty (this uses local proxy mode).

## 5. Complete setup

Finish the setup wizard using either:

- `Remote (OIDC)`, or
- `Remote (Trusted Proxy)` if your HTTPS origin is already behind an identity-aware proxy

## CORS and origin notes

- Default CORS origins already include `https://ci.oore.build`.
- If you set custom origins, include every UI origin you use via `OORE_CORS_ORIGINS`.
- Keep hosted UI mode aligned with the platform contract: `ci.oore.build` is UI-only; your backend runs on your own macOS host.

## Next step

Continue with [Set Up Your Instance](/getting-started/first-instance) for the full setup walkthrough.
