---
status: implemented
---

# Set Up Your Instance

This tutorial walks you through the oore.build setup wizard — from starting the daemon to having a fully configured instance with OIDC authentication.

## What you need

- oore.build [installed](/getting-started/install)
- An OIDC provider configured with a client application (see [Configure OIDC](/guides/oidc/) if you haven't done this yet)
- Your OIDC **issuer URL**, **client ID**, and **client secret** (if required by your provider)

## 1. Start the daemon

Open a terminal and start `oored`:

```bash
~/.oore/bin/oored run --listen 127.0.0.1:8787
```

You should see:

```
INFO oored: using database path="/Users/you/Library/Application Support/oore/oore.db"
INFO oored: database ready instance_id="..." state=BootstrapPending
INFO oored: encryption key ready
INFO oored: starting oored daemon listen=127.0.0.1:8787
```

Leave this terminal running. The daemon listens on `127.0.0.1:8787` by default.

::: tip
Override the listen address with `--listen` or the `OORED_LISTEN_ADDR` environment variable.
:::

## 2. Generate a bootstrap token

In a second terminal, generate a one-time bootstrap token:

```bash
~/.oore/bin/oore setup token --ttl 15m
```

This runs `oore setup token --ttl 15m` and outputs:

```
Bootstrap token generated.

Token:   a1b2c3d4e5f6...
Expires: 2026-02-06 14:30:00 (15m from now)
State:   bootstrap_pending
DB:      /Users/you/Library/Application Support/oore/oore.db

To complete setup, either:
  1. Open https://ci.oore.build and add http://127.0.0.1:8787 as an instance, then continue setup
  2. Run: ~/.oore/bin/oore setup
```

Copy the token value. You'll need it in the next step.

::: warning
The bootstrap token is single-use and expires after its TTL (default: 15 minutes). If it expires, run `~/.oore/bin/oore setup token --ttl 15m` again.
:::

## 3. Complete setup

Choose one of two methods: the **web UI** or the **interactive CLI**.

### Option A: Web UI

1. Open [ci.oore.build](https://ci.oore.build).
2. Add your backend instance URL (`http://127.0.0.1:8787` for local setup).
3. Open the setup flow for that instance.

4. Follow the four steps:

   | Step | What you do |
   |---|---|
   | **1. Bootstrap** | Paste your bootstrap token to authenticate |
   | **2. OIDC** | Enter your issuer URL, client ID, and client secret |
   | **3. Owner** | Sign in with your OIDC provider to create the owner account |
   | **4. Finalize** | Confirm to lock setup endpoints permanently |

### Option B: Interactive CLI

Run the setup command:

```bash
~/.oore/bin/oore setup
```

The CLI walks through the same four steps:

```
oore setup -- interactive instance configuration

Connected to oored at http://127.0.0.1:8787
Instance:  a1b2c3d4-...
State:     bootstrap_pending

[Step 1/4] Bootstrap token verification
  Generating bootstrap token (TTL: 15m)...
  Verifying token with daemon...
  > Bootstrap verified. Session token acquired.

[Step 2/4] OIDC provider configuration
  OIDC Issuer URL: https://accounts.google.com
  Client ID: your-client-id.apps.googleusercontent.com
  Client Secret (optional, press Enter to skip): ****
  > OIDC provider configured.

[Step 3/4] Owner account setup
  Continue with OIDC authentication? [y/N] y
  Waiting for authentication callback...
  > Owner verified: admin@example.com

[Step 4/4] Finalize setup
  Complete setup? This will lock all setup endpoints. [y/N] y
  > Setup complete! Instance ID: a1b2c3d4-...
```

::: info
The CLI OIDC flow opens your default browser and listens on a random local port for the callback. Make sure your OIDC provider's allowed callback URLs include `http://localhost:*` or the specific port shown in the CLI output.
:::

## 4. Verify

Confirm the instance is in `ready` state:

```bash
curl http://127.0.0.1:8787/v1/public/setup-status
```

Expected response:

```json
{
  "instance_id": "550e8400-e29b-41d4-a716-446655440000",
  "state": "ready",
  "setup_mode": false,
  "is_configured": true
}
```

Once `state` is `ready`, setup endpoints are permanently disabled and the instance is ready for normal use.

::: danger
Finalizing setup is irreversible. All setup endpoints (`/v1/setup/*`) are permanently disabled after this step.
:::

## What happens during setup

The instance progresses through four states. Each step requires the previous one to complete:

```
bootstrap_pending → idp_configured → owner_created → ready
```

| State | What happened | What's next |
|---|---|---|
| `bootstrap_pending` | Daemon started, waiting for bootstrap token | Verify token to get a setup session |
| `idp_configured` | OIDC provider discovered and stored | Authenticate owner via OIDC |
| `owner_created` | Owner identity verified, account created | Finalize to lock setup |
| `ready` | Setup complete, all setup endpoints disabled | Instance ready for normal use |

For the full state machine reference, see [Setup States](/reference/setup-states).

## Troubleshooting

### "Cannot reach daemon"

Make sure `oored` is running. Start it with:

```bash
~/.oore/bin/oored run --listen 127.0.0.1:8787
```

### "Setup already complete"

The instance has already been configured. Check with:

```bash
curl http://127.0.0.1:8787/v1/public/setup-status
```

### "OIDC discovery failed"

The daemon couldn't fetch the OpenID Connect discovery document from your issuer URL. Verify that:

- The issuer URL is correct (e.g., `https://accounts.google.com`, not `https://accounts.google.com/`)
- The issuer supports `/.well-known/openid-configuration`
- The daemon has internet access

### "Session expired"

Setup sessions expire after 30 minutes of inactivity. Restart the setup process from step 2.

## Next steps

Your instance is running and authenticated. Continue with:

- [Configure OIDC](/guides/oidc/) — detailed provider setup guides
- [API Reference](/reference/api/) — explore the API
- [CLI Reference](/reference/cli/) — all available commands
