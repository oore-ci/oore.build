# CLI Reference

oore.build provides two command-line tools:

- **`oored`** -- the daemon (control plane and API server)
- **`oore`** -- the operator CLI for setup, administration, and daily use

## oored (Daemon)

The daemon serves the HTTP API and manages instance state.

```bash
oored run              # Start the daemon on 127.0.0.1:8787
oored install-service  # Install as a launchd service (macOS)
oored uninstall-service # Remove the launchd service
oored version          # Print version information
```

### Runtime

- Listens on `127.0.0.1:8787` by default
- Reads state from SQLite at `~/Library/Application Support/oore/oore.db`
- Encryption key stored at `~/Library/Application Support/oore/encryption.key`
- Requires macOS in V1

## oore (Operator CLI)

The operator CLI handles setup, authentication, and administration.

### Command Overview

| Command | Description |
|---|---|
| [`oore setup`](/cli/setup) | Interactive 4-step instance setup |
| [`oore setup open`](/cli/setup#setup-open) | Generate a bootstrap token |
| `oore login` | Authenticate via OIDC |
| `oore status` | Show instance status |
| `oore runner register` | Register an external build runner |
| `oore runner start` | Start external runner process |
| `oore config set <key> <value>` | Set a configuration value |
| `oore config get <key>` | Get a configuration value |
| `oore doctor` | Run diagnostic checks |

::: info
Commands marked as "placeholder" are defined in the CLI structure but not yet fully implemented. The `setup` command is fully implemented.
:::

::: info
Single-host default flow does not require `oore runner start`. `oored` auto-starts an embedded local runner unless `OORED_RUNNER_MODE=external`.
:::

### Global Behavior

- All commands that communicate with the daemon use `--daemon-url` (default: `http://127.0.0.1:8787`, env: `OORE_DAEMON_URL`)
- State database path can be overridden with `--state-file` or `OORE_SETUP_STATE_FILE` env var
- The default database location is `~/Library/Application Support/oore/oore.db`
