---
status: implemented
description: 'Register and manage the Direct macOS runner for trusted repositories.'
---

# Run Builds with the Direct macOS Runner

Oore V1 executes builds through a separate `oore-runner` process on macOS. Repository commands run directly with the permissions of the runner's macOS account, so approve only repositories whose code and contributors you would run on that Mac yourself.

The Direct runner is a compatibility-first execution mode, not a hostile-code sandbox. Oore does not automatically run external-fork pull or merge requests in this mode.

## What you need

- **Role**: admin or owner (for registration)
- A ready Oore CI daemon
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

The separate service keeps runner lifecycle and updates independent from the daemon. It is not an OS security boundary between repository code and the runner account.

## 3. Verify

1. Go to **Settings > Preferences** and enable **Direct macOS runner**
2. Go to **Settings > Sources**, open the source, and approve the repository
3. Go to **Settings > Runners** and confirm the runner is `online`
4. Trigger a test build and verify it is picked up

Approval is repository-wide: every Oore project linked to that repository shares the same decision. New and newly re-added repositories start unapproved. Turning off the instance switch or revoking repository approval lets running builds finish while queued builds wait.

## Recommended account setup

Use a dedicated, non-admin macOS account for the runner when practical, and keep operator tokens and unrelated personal credentials out of that account. This reduces accidental exposure, but it does not turn Direct mode into isolation. Strong isolation for untrusted code requires a disposable VM and is not part of V1.

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
