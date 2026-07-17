---
status: implemented
description: 'CLI reference for the oore operator tool including all commands and flags.'
---

# CLI Reference

Oore CI provides two command-line tools:

- **`oored`** — the daemon (control plane and API server)
- **`oore`** — the operator CLI for setup, administration, and daily use

Validate repository pipeline YAML locally with the exact parser used by the daemon and runner:

```bash
oore pipeline validate .oore.yaml
```

## oored (Daemon)

The daemon serves the HTTP API and manages instance state.

### Commands

| Command                   | Description                                               | Status      |
| ------------------------- | --------------------------------------------------------- | ----------- |
| `oored run`               | Start the daemon                                          | Implemented |
| `oored install-service`   | Install and optionally start a macOS launchd user service | Implemented |
| `oored uninstall-service` | Stop and remove the launchd user service                  | Implemented |
| `oored version`           | Print version information                                 | Implemented |

### `oored run`

```bash
oored run [--listen <addr>] [--state-file <path>]
```

| Flag           | Default          | Env var                 | Description                   |
| -------------- | ---------------- | ----------------------- | ----------------------------- |
| `--listen`     | `127.0.0.1:8787` | `OORED_LISTEN_ADDR`     | Address and port to listen on |
| `--state-file` | Platform default | `OORE_SETUP_STATE_FILE` | Override database path        |

The default database path is `~/Library/Application Support/oore/oore.db`. The encryption key is stored at `~/Library/Application Support/oore/encryption.key`.

`oored` is the control plane only. Builds require a registered Direct macOS runner process; omitting `OORED_RUNNER_MODE` and setting it to `external` are equivalent. Legacy `embedded` and `hybrid` values fail closed.

### `oored install-service`

```bash
oored install-service [--listen <addr>] [--state-file <path>] [--label <label>] [--env KEY=VALUE] [--no-start] [--system --user <name>]
```

Installs `oored` as a macOS launchd user service. The default service label is
`build.oore.oored`, and the plist is written to
`~/Library/LaunchAgents/build.oore.oored.plist`.

| Flag              | Default            | Env var                 | Description                                                                                 |
| ----------------- | ------------------ | ----------------------- | ------------------------------------------------------------------------------------------- |
| `--listen`        | `127.0.0.1:8787`   | `OORED_LISTEN_ADDR`     | Address and port used by the service                                                        |
| `--state-file`    | Platform default   | `OORE_SETUP_STATE_FILE` | Override database path                                                                      |
| `--label`         | `build.oore.oored` | none                    | Override the launchd service label and plist name                                           |
| `--env KEY=VALUE` | none               | none                    | Add or override an environment variable in the launchd plist. Repeat for multiple variables |
| `--no-start`      | `false`            | none                    | Write the plist without bootstrapping the service                                           |
| `--system`        | `false`            | none                    | Install a boot-time LaunchDaemon; requires root                                             |
| `--user`          | none               | none                    | Account that runs the LaunchDaemon; required with `--system`                                |

The service uses the currently running `oored` executable, keeps the daemon alive
with launchd, and writes logs to `~/.oore/logs/oored.log`. It does not execute
repository commands; install the runner's separate login-session service after
registering it.

Examples:

```bash
# Persistent local daemon on loopback
oored install-service --listen 127.0.0.1:8787

# Production reverse-proxy deployment
oored install-service \
  --listen 127.0.0.1:8787 \
  --env OORE_PUBLIC_URL=https://ci.mycompany.com \
  --env OORE_CORS_ORIGINS=https://ci.mycompany.com

# Custom database path
oored install-service \
  --state-file "$HOME/Library/Application Support/oore-prod/oore.db"
```

Useful launchd checks:

```bash
launchctl print gui/$(id -u)/build.oore.oored
sudo launchctl print system/build.oore.oored
tail -f ~/.oore/logs/oored.log
```

### `oored uninstall-service`

```bash
oored uninstall-service [--label <label>] [--system]
```

Stops and removes the launchd user service. This deletes the plist but leaves
the database, encryption key, and logs untouched.

## oore (Operator CLI)

The operator CLI handles setup, authentication, and administration.

### Commands

| Command                                                       | Description                                             | Status      |
| ------------------------------------------------------------- | ------------------------------------------------------- | ----------- |
| [`oore setup`](/reference/cli/oore-setup)                     | Interactive 4-step instance setup                       | Implemented |
| [`oore setup token`](/reference/cli/oore-setup#setup-token)   | Generate a bootstrap token                              | Implemented |
| [`oore login`](/reference/cli/oore-login)                     | Authenticate in local mode or import a token            | Implemented |
| [`oore recovery`](/reference/cli/oore-recovery)               | Mint a one-use browser recovery link on the daemon host | Implemented |
| [`oore status`](/reference/cli/oore-status)                   | Show setup status and authenticated operational summary | Implemented |
| `oore runner register`                                        | Register an external build runner                       | Implemented |
| `oore runner start`                                           | Start external runner process                           | Implemented |
| `oore runner install-service`                                 | Install a macOS login-session runner service            | Implemented |
| `oore runner uninstall-service`                               | Remove the managed macOS runner service                 | Implemented |
| [`oore config set <key> <value>`](/reference/cli/oore-config) | Set CLI configuration values                            | Implemented |
| [`oore config get <key>`](/reference/cli/oore-config)         | Get CLI configuration values                            | Implemented |
| [`oore doctor`](/reference/cli/oore-doctor)                   | Run environment/signing diagnostics                     | Implemented |
| `oore backup create                                           | verify                                                  | restore`    | Create and recover verified SQLite/key backups | Implemented |
| `oore update`                                                 | Safely install a verified release update                | Implemented |

### Global behavior

- All commands that communicate with the daemon accept `--daemon-url` (default: `http://127.0.0.1:8787`, env: `OORE_DAEMON_URL`)
- The CLI stores local defaults in `~/.oore/config.json` (override path via `OORE_CONFIG_FILE`)
- State database path can be overridden with `--state-file` or `OORE_SETUP_STATE_FILE`
- Default database location: `~/Library/Application Support/oore/oore.db`

### Direct macOS runner note

All builds use the separate Direct macOS runner. Register it once with
`oore runner register`, then install its managed login-session service with
`oore runner install-service`. This keeps normal builds and Apple Keychain code
signing in the runner user's macOS session. After an update, Oore's managed
updater restarts that runner service automatically.
