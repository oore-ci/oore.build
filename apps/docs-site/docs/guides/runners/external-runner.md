---
status: implemented
description: 'Register and manage external build runners for Oore CI.'
---

# Register an External Runner

For builds on a separate machine or for isolating the build environment, register an external runner.

## What you need

- **Role**: admin or owner (for registration)
- The Oore CI daemon running with `OORED_RUNNER_MODE=external` (or in addition to the embedded runner)
- A user session token for registration
- The runner machine must have all [prerequisites](/getting-started/prerequisites) installed

## 1. Register the runner

On the runner machine:

```bash
oore runner register \
  --daemon-url https://<daemon-host> \
  --token <session_token> \
  --name "mac-mini-builder"
```

| Flag           | Default                 | Env var              | Description                           |
| -------------- | ----------------------- | -------------------- | ------------------------------------- |
| `--daemon-url` | `http://127.0.0.1:8787` | `OORE_DAEMON_URL`    | Daemon URL                            |
| `--token`      | —                       | `OORE_SESSION_TOKEN` | User session token for authentication |
| `--name`       | —                       | —                    | Display name for the runner           |

This creates a runner record on the daemon and writes a config file at `~/.oore/runner.json` with the runner's token.

Runner registration and runtime traffic require HTTPS whenever the daemon is
not addressed by a literal loopback IP. Cleartext HTTP remains available only
for `127.0.0.1` or `::1`; hostnames such as `localhost` are intentionally not
treated as proof of loopback.

## 2. Start the runner

On macOS, install the managed user service:

```bash
oore runner install-service
```

The service starts at interactive login and remains running in the user's Aqua session. This is the supported mode for iOS signing because Apple Keychain Services does not expose imported private keys to a system LaunchDaemon or a background-only login session.

For temporary foreground use instead:

```bash
oore runner start
```

| Flag           | Default               | Description                   |
| -------------- | --------------------- | ----------------------------- |
| `--daemon-url` | From config           | Daemon URL (overrides config) |
| `--config`     | `~/.oore/runner.json` | Runner config file path       |

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

| State      | Meaning                                                    |
| ---------- | ---------------------------------------------------------- |
| `online`   | Runner is healthy, sending heartbeats, ready for jobs      |
| `busy`     | Runner is executing a build                                |
| `offline`  | Runner hasn't sent a heartbeat recently                    |
| `draining` | Runner is finishing current work and won't accept new jobs |

## Stopping the runner

For a foreground runner, stop `oore runner start` with Ctrl+C. For the managed macOS service, run:

```bash
oore runner uninstall-service
```

The daemon marks the runner as `offline` after the heartbeat timeout.

## API endpoints

| Method | Path                                | Description              |
| ------ | ----------------------------------- | ------------------------ |
| `POST` | `/v1/runners/register`              | Register a new runner    |
| `GET`  | `/v1/runners`                       | List all runners         |
| `POST` | `/v1/runners/{runner_id}/heartbeat` | Send heartbeat           |
| `POST` | `/v1/runners/{runner_id}/claim`     | Claim next available job |
