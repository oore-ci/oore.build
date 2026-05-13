---
status: implemented
description: "CLI reference for the oore operator tool including all commands and flags."
---

# CLI Reference

Oore CI provides two command-line tools:

- **`oored`** — the daemon (control plane and API server)
- **`oore`** — the operator CLI for setup, administration, and daily use

## oored (Daemon)

The daemon serves the HTTP API and manages instance state.

### Commands

| Command | Description | Status |
|---|---|---|
| `oored run` | Start the daemon | Implemented |
| `oored install-service` | Install as a macOS launchd service | Placeholder |
| `oored uninstall-service` | Remove the launchd service | Placeholder |
| `oored version` | Print version information | Implemented |

### `oored run`

```bash
oored run [--listen <addr>] [--state-file <path>]
```

| Flag | Default | Env var | Description |
|---|---|---|---|
| `--listen` | `127.0.0.1:8787` | `OORED_LISTEN_ADDR` | Address and port to listen on |
| `--state-file` | Platform default | `OORE_SETUP_STATE_FILE` | Override database path |

The default database path is `~/Library/Application Support/oore/oore.db`. The encryption key is stored at `~/Library/Application Support/oore/encryption.key`.

In default mode, `oored` starts an embedded local runner automatically. Set `OORED_RUNNER_MODE=external` to disable the embedded runner and require an external runner process.

## oore (Operator CLI)

The operator CLI handles setup, authentication, and administration.

### Commands

| Command | Description | Status |
|---|---|---|
| [`oore setup`](/reference/cli/oore-setup) | Interactive 4-step instance setup | Implemented |
| [`oore setup token`](/reference/cli/oore-setup#setup-token) | Generate a bootstrap token | Implemented |
| [`oore login`](/reference/cli/oore-login) | Authenticate in local mode or import a token | Implemented |
| [`oore status`](/reference/cli/oore-status) | Show setup status and authenticated operational summary | Implemented |
| [`oore external-access enable-trusted-proxy`](/reference/cli/oore-external-access) | Configure public URL, allowed origins, trusted proxy peers, and remote trusted-proxy mode | Implemented |
| [`oore users transfer-owner`](/reference/cli/oore-users) | Transfer the singleton owner role to an existing active user | Implemented |
| `oore runner register` | Register an external build runner | Implemented |
| `oore runner start` | Start external runner process | Implemented |
| [`oore config set <key> <value>`](/reference/cli/oore-config) | Set CLI configuration values | Implemented |
| [`oore config get <key>`](/reference/cli/oore-config) | Get CLI configuration values | Implemented |
| [`oore doctor`](/reference/cli/oore-doctor) | Run environment/signing diagnostics | Implemented |

### Global behavior

- All commands that communicate with the daemon accept `--daemon-url` (default: `http://127.0.0.1:8787`, env: `OORE_DAEMON_URL`)
- The CLI stores local defaults in `~/.oore/config.json` (override path via `OORE_CONFIG_FILE`)
- State database path can be overridden with `--state-file` or `OORE_SETUP_STATE_FILE`
- Default database location: `~/Library/Application Support/oore/oore.db`

### Embedded runner note

The single-host default flow does not require `oore runner start`. The daemon (`oored`) auto-starts an embedded local runner unless `OORED_RUNNER_MODE=external`.
