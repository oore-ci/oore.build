---
status: implemented
description: 'Register and manage the Direct macOS runner for trusted repositories.'
---

# Run Builds with the Direct macOS Runner

Oore V1 executes builds through a separate `oore-runner` process on macOS. Repository commands run directly with the permissions of the runner's macOS account, so create projects only for repositories whose code and contributors you would run on that Mac yourself.

The Direct runner is a compatibility-first execution mode, not a hostile-code sandbox. Oore does not automatically run external-fork pull or merge requests in this mode.

## What you need

- **Role**: admin or owner (for registration)
- A ready Oore CI daemon
- A user session token for registration
- The runner machine must have all [prerequisites](/getting-started/prerequisites) installed

## Managed runner on the backend Mac

The macOS `all` and `backend` installers enroll the local runner and install it
with the backend service. Both processes are boot-time system LaunchDaemons, so
they recover after a restart without waiting for a GUI login.

To repair or replace the managed runner service, run this as the macOS account
that should execute builds (not with `sudo`):

```bash
oore runner install-service --managed-local
```

Oore requests administrator access only to install the LaunchDaemon. When the
runner and daemon share the local Oore database, the command creates or repairs
the local runner registration and its `~/.oore/managed-runner.json` config
automatically.

If this host is upgrading from an earlier login-session daemon or runner, rerun
the current installer for the host's installed release channel. The installer
uses the verified candidate updater to drain active work, migrate both services,
verify their restart, and roll back the release, data, and service definitions
if the transition fails. Afterward, reboot recovery and managed updates are
automatic.

The managed local runner uses `~/.oore/managed-runner.json`, so an existing
manual external registration in `~/.oore/runner.json` is left untouched.

## Manual runner on another Mac

Manual registration remains available when the runner is on a different Mac
from the backend.

### 1. Register the runner

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

### 2. Install the runner service

Run this as the macOS account that should execute builds:

```bash
oore runner install-service
```

The command installs a system LaunchDaemon that starts at boot and runs as that
non-root account. It keeps the same `HOME`, toolchain paths, workspace ownership,
and file permissions as a foreground runner.

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

The separate service keeps runner execution out of the control-plane process.
Backend restarts do not stop it, and managed updates restart it when its binary
changes. It is not an OS security boundary between repository code and the
runner account.

## Verify

1. Go to **Settings > Runners** and confirm the runner is `online`
2. In **Settings > Preferences**, confirm **Accept new builds** is on
3. Trigger a test build and verify it is picked up

Creating a project or changing its linked source is the execution trust decision
and therefore requires an Owner or Admin. There is no second per-repository
allowlist to maintain. Turning off **Accept new builds** is an operational pause:
running builds finish while queued builds wait.

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

For a foreground runner, stop `oore runner start` with Ctrl+C. To remove the
managed macOS service, run this as its runner account:

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
