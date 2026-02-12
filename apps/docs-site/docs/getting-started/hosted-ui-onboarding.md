---
status: implemented
---

# Hosted UI Onboarding

Use this guide after installing backend binaries to complete setup from `https://ci.oore.build`.

## 1. Start the daemon

```bash
~/.oore/bin/oored run --listen 0.0.0.0:8787
```

Use `127.0.0.1:8787` for local-only testing, or a reachable host/IP for remote browser access.

## 2. Confirm backend health

```bash
curl http://127.0.0.1:8787/healthz
curl http://127.0.0.1:8787/v1/public/setup-status
```

## 3. Generate a setup token

```bash
~/.oore/bin/oore setup token --ttl 15m
```

Keep this token ready for the setup wizard.

## 4. Open hosted UI and add your backend

1. Open [ci.oore.build](https://ci.oore.build).
2. Use **Add Instance**.
3. Enter your backend URL:
   - Local testing: `http://127.0.0.1:8787`
   - Network-reachable host: `https://ci.your-company.internal`
4. Continue to `/setup` and paste the bootstrap token.

## 5. Complete OIDC setup

Finish the OIDC configuration and owner verification flow in the setup wizard.

## CORS and origin notes

- Default CORS origins already include `https://ci.oore.build`.
- If you set custom origins, include every UI origin you use via `OORE_CORS_ORIGINS`.
- Keep hosted UI mode aligned with the platform contract: `ci.oore.build` is UI-only; your backend runs on your own macOS host.

## Next step

Continue with [Set Up Your Instance](/getting-started/first-instance) for the full setup walkthrough.
