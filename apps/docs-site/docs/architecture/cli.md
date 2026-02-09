# CLI Architecture

The `oore` CLI is the operator-facing tool for instance setup, administration, and runner management. It communicates with the `oored` daemon over HTTP.

## Command structure

The CLI is built with **Clap 4.5** using the derive API. Commands follow a noun-verb pattern:

```
oore <command> [subcommand] [options]
```

### Available commands

| Command | Subcommand | Description | Status |
|---------|------------|-------------|--------|
| `setup` | `open` | Generate a time-bound bootstrap token | Implemented |
| `setup` | *(none)* | Interactive setup wizard | Implemented |
| `login` | | Authenticate with the daemon | Placeholder |
| `status` | | Check daemon and instance status | Placeholder |
| `runner` | `register` | Register an external build runner | Implemented |
| `runner` | `start` | Start external runner process | Implemented |
| `config` | `set` | Set a configuration value | Placeholder |
| `config` | `get` | Get a configuration value | Placeholder |
| `doctor` | | Run diagnostic checks | Placeholder |

::: info
The `oored` and `oore` command names are **stable contract surfaces**. New commands may be added, but existing command names and core semantics remain backward compatible in V1.
:::

## Setup flow

The CLI supports two setup modes: **token generation** and **interactive setup**.

### Token generation (`oore setup open`)

Generates a one-time bootstrap token and writes it to the shared SQLite database.

```bash
oore setup open --ttl 15m
```

Options:

| Flag | Default | Description |
|------|---------|-------------|
| `--ttl` | `15m` | Token time-to-live (human-readable duration) |
| `--json` | `false` | Output in JSON format |
| `--state-file` | Auto-detected | Override database path |

The token is stored as a SHA-256 hash in the database. The plaintext is displayed to the operator and never persisted.

### Interactive setup (`oore setup`)

A guided four-step setup wizard that communicates with a running `oored` daemon:

```
Step 1: Bootstrap token verification
  └── Generates token, verifies against daemon, acquires session

Step 2: OIDC provider configuration
  └── Prompts for issuer URL, client ID, client secret
  └── Sends to daemon for OIDC discovery validation

Step 3: Owner account setup (OIDC loopback)
  └── Opens browser for OIDC authentication
  └── Listens on random local port for callback
  └── Exchanges code for tokens, creates owner

Step 4: Finalize setup
  └── Confirms with operator, locks setup endpoints
```

## OIDC loopback flow

The CLI uses a **loopback OIDC flow** for owner verification during setup. This allows the operator to authenticate with their identity provider directly from the terminal.

```
┌─────────┐     ┌──────────┐     ┌──────────────┐     ┌────────┐
│  oore   │     │  oored   │     │  OIDC        │     │  User  │
│  CLI    │     │  daemon  │     │  Provider    │     │Browser │
└────┬────┘     └────┬─────┘     └──────┬───────┘     └───┬────┘
     │               │                  │                  │
     │ POST start-oidc                  │                  │
     │──────────────▶│                  │                  │
     │               │                  │                  │
     │ auth_url      │                  │                  │
     │◀──────────────│                  │                  │
     │               │                  │                  │
     │ open browser ─────────────────────────────────────▶│
     │               │                  │                  │
     │ bind 127.0.0.1:random_port      │     login page  │
     │               │                  │◀─────────────────│
     │               │                  │                  │
     │               │                  │  credentials    │
     │               │                  │◀─────────────────│
     │               │                  │                  │
     │  callback (code, state)          │  redirect       │
     │◀──────────────────────────────────────────────────── │
     │               │                  │                  │
     │ POST verify-oidc (code, state)   │                  │
     │──────────────▶│                  │                  │
     │               │ exchange code    │                  │
     │               │─────────────────▶│                  │
     │               │ tokens           │                  │
     │               │◀─────────────────│                  │
     │               │                  │                  │
     │  owner created│                  │                  │
     │◀──────────────│                  │                  │
```

Key implementation details:

1. The CLI binds to `127.0.0.1:0` (random free port) using a raw `TcpListener`
2. The redirect URI is `http://localhost:<port>` -- this must be whitelisted in the OIDC provider
3. After the callback, the CLI parses the HTTP GET request to extract `code` and `state` query parameters
4. A success or error HTML page is rendered in the browser
5. The CLI then calls `POST /v1/setup/owner/verify-oidc` on the daemon to exchange the code

::: tip
The CLI uses `open` (macOS) to launch the browser. This is macOS-only, matching the V1 backend platform requirement.
:::

## Daemon communication

The CLI connects to the daemon at `http://127.0.0.1:8787` by default. Override with:

```bash
oore setup --daemon-url http://192.168.1.100:8787
```

Or via environment variable:

```bash
export OORE_DAEMON_URL=http://192.168.1.100:8787
```

## Shared database

Both `oored` and `oore` share the same SQLite database for setup state. The database path is resolved in priority order:

1. `--state-file` CLI flag
2. `OORE_SETUP_STATE_FILE` environment variable
3. Default: `~/Library/Application Support/oore/oore.db`

::: warning
The CLI writes the bootstrap token hash directly to the database. The daemon reads from the same database to verify the token. Both processes must have access to the same database file.
:::

## Error handling

The CLI handles daemon API errors by parsing the structured `ApiError` response:

```json
{
  "error": "Bootstrap token has already been consumed",
  "code": "token_consumed",
  "details": null
}
```

Specific HTTP status codes trigger different behaviors:

| Status | Behavior |
|--------|----------|
| `409 Conflict` | Setup already complete, exit gracefully |
| `401 Unauthorized` | Session expired, prompt to restart |
| `4xx` / `5xx` | Display error, offer retry where appropriate |

## Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` 4.5 | Command parsing (derive API) |
| `reqwest` | HTTP client for daemon communication |
| `dialoguer` | Interactive terminal prompts |
| `sqlx` | Direct SQLite access for token generation |
| `humantime` | TTL duration parsing |
| `tokio` | Async runtime |
| `oore-contract` | Shared types with the daemon |
