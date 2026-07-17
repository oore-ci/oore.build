---
status: implemented
description: 'Environment variables for configuring the Oore CI daemon and CLI.'
---

# Environment Variables

All environment variables recognized by Oore CI components.

## Daemon (oored)

| Variable                     | Default                                                                                   | Description                                                                                     |
| ---------------------------- | ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `OORED_LISTEN_ADDR`          | `127.0.0.1:8787`                                                                          | Address and port for the daemon to listen on                                                    |
| `OORED_RUNNER_MODE`          | `external`                                                                                | Legacy compatibility switch: omit it or use `external`; `embedded` and `hybrid` are rejected    |
| `OORED_DATA_DIR`             | Platform default                                                                          | Override daemon data root directory (`oore.db`, `encryption.key`, local artifacts default path) |
| `OORE_DATA_DIR`              | Platform default                                                                          | Shared fallback data root (used when `OORED_DATA_DIR` is unset)                                 |
| `OORE_SETUP_STATE_FILE`      | Platform default                                                                          | Override SQLite database path                                                                   |
| `OORE_PUBLIC_URL`            | —                                                                                         | External Access public base URL fallback (used when DB setting is unset)                        |
| `OORE_ARTIFACT_DELIVERY_URL` | `OORE_PUBLIC_URL`                                                                         | Optional alternate HTTPS origin for `/install/` delivery (used when the DB field is empty)      |
| `OORE_WARPGATE_TICKET`       | —                                                                                         | Optional Warpgate access ticket fallback for non-interactive iOS OTA installer requests         |
| `OORE_CORS_ORIGINS`          | `http://localhost:3000,http://127.0.0.1:3000,http://localhost:4173,http://127.0.0.1:4173` | Comma-separated allowed CORS origins fallback (used when DB settings are unset)                 |
| `OORE_CORS_ORIGIN`           | —                                                                                         | Single allowed CORS origin fallback (backward compatible)                                       |
| `RUST_LOG`                   | `info`                                                                                    | Log level filter (uses `tracing` subscriber)                                                    |

Data root resolution order: `OORED_DATA_DIR` -> `OORE_DATA_DIR` -> `~/Library/Application Support/oore`

Default database path: `<data-root>/oore.db`

Default encryption key path: `<data-root>/encryption.key`
Default local artifact path: `<data-root>/artifacts`

External Access network settings are primarily managed from Preferences UI and
stored in SQLite. The environment variables above are used as fallback defaults
when DB-backed settings are not present.

`OORE_WARPGATE_TICKET` is active only when the instance is in Remote mode with
Trusted Proxy auth and the identity header is `x-warpgate-username`. An owner can
instead save the ticket in **Preferences → Identity settings**, where Oore stores
it encrypted; that database value takes precedence over the environment fallback.
Oore never applies this ticket to Android downloads or non-Warpgate deployments.

## CLI (oore)

| Variable                | Default                 | Description                                    |
| ----------------------- | ----------------------- | ---------------------------------------------- |
| `OORE_DAEMON_URL`       | `http://127.0.0.1:8787` | Daemon URL for CLI commands                    |
| `OORED_DATA_DIR`        | Platform default        | Shared DB root override for CLI setup commands |
| `OORE_DATA_DIR`         | Platform default        | Shared DB root override for CLI setup commands |
| `OORE_SETUP_STATE_FILE` | Platform default        | Override SQLite database path                  |
| `OORE_SESSION_TOKEN`    | —                       | Session token for `oore runner register`       |

## Runner

Environment variables set by the runner during build execution:

| Variable                 | Description                                                        |
| ------------------------ | ------------------------------------------------------------------ |
| `OORE_KEYSTORE_PATH`     | Path to Android keystore file (when Android signing is configured) |
| `OORE_KEYSTORE_PASSWORD` | Android keystore password                                          |
| `OORE_KEY_ALIAS`         | Android key alias                                                  |
| `OORE_KEY_PASSWORD`      | Android key password                                               |

Additionally, any environment variables defined in the pipeline's `env` configuration are set during builds.

## Pipeline configuration

Environment variables can be set per-pipeline in `.oore.yaml`:

```yaml
env:
  - key: JAVA_HOME
    value: /usr/local/opt/openjdk@17
  - key: MY_BUILD_VAR
    value: custom-value
```

Or via the pipeline editor in the web UI.
