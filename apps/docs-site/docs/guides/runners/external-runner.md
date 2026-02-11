---
status: implemented
---

# Register an External Runner

For builds on a separate machine or for isolating the build environment, register an external runner.

## What you need

- **Role**: admin or owner (for registration)
- The oore.build daemon running with `OORED_RUNNER_MODE=external` (or in addition to the embedded runner)
- A user session token for registration
- The runner machine must have all [prerequisites](/getting-started/prerequisites) installed

## 1. Register the runner

On the runner machine:

```bash
oore runner register \
  --daemon-url http://<daemon-host>:8787 \
  --token <session_token> \
  --name "mac-mini-builder"
```

| Flag | Default | Env var | Description |
|---|---|---|---|
| `--daemon-url` | `http://127.0.0.1:8787` | `OORE_DAEMON_URL` | Daemon URL |
| `--token` | — | `OORE_SESSION_TOKEN` | User session token for authentication |
| `--name` | — | — | Display name for the runner |

This creates a runner record on the daemon and writes a config file at `~/.oore/runner.json` with the runner's token.

## 2. Start the runner

```bash
oore runner start
```

| Flag | Default | Description |
|---|---|---|
| `--daemon-url` | From config | Daemon URL (overrides config) |
| `--config` | `~/.oore/runner.json` | Runner config file path |

The runner process:

1. Reads the config file for its token and daemon URL
2. Sends periodic heartbeats to the daemon
3. Polls for available jobs
4. Executes claimed builds

## 3. Verify

1. Go to **Settings > Runners** in the web UI
2. The external runner should appear as `online`
3. Trigger a test build and verify it's picked up

## Runner lifecycle

| State | Meaning |
|---|---|
| `online` | Runner is healthy, sending heartbeats, ready for jobs |
| `busy` | Runner is executing a build |
| `offline` | Runner hasn't sent a heartbeat recently |
| `draining` | Runner is finishing current work and won't accept new jobs |

## Stopping the runner

Stop the `oore runner start` process (Ctrl+C or kill the process). The daemon marks it as `offline` after heartbeat timeout.

## API endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/v1/runners/register` | Register a new runner |
| `GET` | `/v1/runners` | List all runners |
| `POST` | `/v1/runners/{runner_id}/heartbeat` | Send heartbeat |
| `POST` | `/v1/runners/{runner_id}/claim` | Claim next available job |
