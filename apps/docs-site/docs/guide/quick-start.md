# Quick Start

This guide walks you through starting the oore.build daemon, running the setup wizard, and getting to your first executable build pipeline.

## 1. Start the daemon

Start `oored` in a terminal. It will initialize the SQLite database and listen for API requests.

```bash
make run-daemon
```

You should see output like:

```
INFO oored: using database path="/Users/you/Library/Application Support/oore/oore.db"
INFO oored: database ready instance_id="a1b2c3d4-..." state=BootstrapPending
INFO oored: encryption key ready path="/Users/you/Library/Application Support/oore/encryption.key"
INFO oored: starting oored daemon listen=127.0.0.1:8787
```

In default mode, `oored` also starts an embedded local runner so queued builds can execute without a separate `oore runner start` process.

::: tip
The daemon defaults to `127.0.0.1:8787`. Override this with the `--listen` flag or `OORED_LISTEN_ADDR` environment variable.
:::

## 2. Generate a bootstrap token

In a second terminal, generate a one-time bootstrap token using the CLI:

```bash
make run-cli
```

This runs `oore setup open --ttl 15m`, which produces:

```
Bootstrap token generated.

Token:   a1b2c3d4e5f6...
Expires: 2026-02-06 14:30:00 (15m from now)
State:   bootstrap_pending
DB:      /Users/you/Library/Application Support/oore/oore.db

To complete setup, either:
  1. Open http://localhost:3000/setup in your browser and paste this token
  2. Run: oore setup
```

::: warning
The bootstrap token is one-time use and expires after the TTL (default: 15 minutes). If it expires, generate a new one.
:::

## 3. Complete setup

You have two options to complete the setup: the **web UI** or the **interactive CLI**.

### Option A: Web UI setup wizard

1. Start the web dev server:

   ```bash
   make dev-web
   ```

2. Open `http://localhost:3000/setup` in your browser.

3. Follow the setup wizard:
   - **Step 1** -- Paste your bootstrap token to authenticate
   - **Step 2** -- Configure your OIDC provider (issuer URL, client ID, and optionally a client secret)
   - **Step 3** -- Verify your identity via OIDC to create the owner account
   - **Step 4** -- Finalize setup to lock all setup endpoints

### Option B: Interactive CLI setup

Run the interactive setup flow directly from the terminal:

```bash
cargo run -p oore -- setup
```

The CLI will guide you through the same four steps:

```
oore setup -- interactive instance configuration

Connected to oored at http://127.0.0.1:8787
Instance:  a1b2c3d4-...
State:     bootstrap_pending

[Step 1/4] Bootstrap token verification
  Generating bootstrap token (TTL: 15m)...
  Verifying token with daemon...
  ✓ Bootstrap verified. Session token acquired.

[Step 2/4] OIDC provider configuration
  OIDC Issuer URL: https://accounts.google.com
  Client ID: your-client-id.apps.googleusercontent.com
  Client Secret (optional, press Enter to skip): ****
  ✓ OIDC provider configured.

[Step 3/4] Owner account setup
  Continue with OIDC authentication? [y/N] y
  Waiting for authentication callback...
  ✓ Owner verified: admin@example.com

[Step 4/4] Finalize setup
  Complete setup? This will lock all setup endpoints. [y/N] y
  ✓ Setup complete! Instance ID: a1b2c3d4-...
```

::: info
The CLI OIDC flow opens your default browser and listens on a random local port for the callback. Make sure the loopback redirect URI (`http://localhost:<port>`) is whitelisted in your OIDC provider's allowed callback URLs.
:::

## 4. Verify the instance

After setup is complete, check the instance status:

```bash
curl http://127.0.0.1:8787/v1/public/setup-status | jq
```

Expected response:

```json
{
  "instance_id": "a1b2c3d4-...",
  "state": "ready",
  "setup_mode": false,
  "is_configured": true
}
```

Once the state is `ready`, the setup endpoints are permanently disabled and the instance is ready for normal operation.

## 5. Verify build execution path

1. Create a project/pipeline and trigger a build from the UI.
2. Confirm build state moves out of `queued` within a few seconds.
3. Open **Settings -> Runners** to confirm the runner status and heartbeat.

If the build stays queued, check whether `OORED_RUNNER_MODE` is set to `external`. In `external` mode, start a runner manually (`make run-runner`) or switch back to default embedded mode.

## Setup state machine

The instance transitions through these states during setup:

```
bootstrap_pending ──▶ idp_configured ──▶ owner_created ──▶ ready
       │                    │                  │               │
   Generate &           Configure          Verify owner    Lock setup
   verify token         OIDC provider      via OIDC        endpoints
```

Each step is gated by the previous one, and setup endpoints require a valid session token obtained from the bootstrap verification step.

## What's next

- [Architecture overview](/architecture/overview) -- understand the workspace layout and tech choices
- [Backend architecture](/architecture/backend) -- deep dive into the daemon and data layer
- [CLI reference](/cli/overview) -- all available `oore` and `oored` commands
