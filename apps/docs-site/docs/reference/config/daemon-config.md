---
status: implemented
description: 'Configuration options for the oored daemon process.'
---

# Daemon Configuration

Runtime configuration for the `oored` daemon.

## Command-line flags

```bash
oored run [--listen <addr>] [--state-file <path>]
```

| Flag           | Default          | Env var                 | Description                   |
| -------------- | ---------------- | ----------------------- | ----------------------------- |
| `--listen`     | `127.0.0.1:8787` | `OORED_LISTEN_ADDR`     | Address and port to listen on |
| `--state-file` | Platform default | `OORE_SETUP_STATE_FILE` | Override database path        |

## Environment variables

See [Environment Variables](/reference/config/environment-variables) for the complete list.

## File locations

| File               | Default path                                        | Description                             |
| ------------------ | --------------------------------------------------- | --------------------------------------- |
| **Database**       | `~/Library/Application Support/oore/oore.db`        | SQLite database with all instance state |
| **Encryption key** | `~/Library/Application Support/oore/encryption.key` | AES-256-GCM key for secret encryption   |

Override the database path with `--state-file` or `OORE_SETUP_STATE_FILE`.

## Runner execution

`oored` does not execute repository commands. Builds require the separate Direct
macOS runner process. `OORED_RUNNER_MODE` may be omitted or set to `external`;
legacy `embedded` and `hybrid` values are rejected.

The runner claims a queued build while **Accept new builds** is on and the
project's linked source is available. The Owner/Admin action that links the
source to a project is the execution trust decision; there is no separate
repository allowlist.

## CORS configuration

| Variable            | Default                                       | Description                         |
| ------------------- | --------------------------------------------- | ----------------------------------- |
| `OORE_CORS_ORIGINS` | `http://localhost:3000,https://ci.oore.build` | Comma-separated allowed origins     |
| `OORE_CORS_ORIGIN`  | —                                             | Single origin (backward compatible) |

## Logging

| Variable   | Default | Description                                |
| ---------- | ------- | ------------------------------------------ |
| `RUST_LOG` | `info`  | Log level filter (uses tracing subscriber) |

Examples:

```bash
RUST_LOG=debug                    # All debug logs
RUST_LOG=oored=debug,tower=info  # Module-specific levels
RUST_LOG=warn                    # Only warnings and errors
```
