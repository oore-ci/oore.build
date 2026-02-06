# Backend Architecture

The oore.build backend consists of two Rust binaries (`oored` and `oore`) and one shared library crate (`oore-contract`). This page covers the daemon's internals.

## Daemon startup

When you run `oored run`, the daemon performs these initialization steps:

1. **Parse CLI args** -- listen address, optional state file path
2. **Resolve database path** -- from `--state-file`, `OORE_SETUP_STATE_FILE` env, or default (`~/Library/Application Support/oore/oore.db`)
3. **Connect to SQLite** -- create the database file and run embedded migrations
4. **Initialize state** -- if no state row exists, create one with `BootstrapPending` and a fresh UUID instance ID
5. **Load encryption key** -- from `~/Library/Application Support/oore/encryption.key` (auto-generated on first run)
6. **Build Axum router** -- mount all route handlers with shared application state
7. **Start HTTP server** -- bind to the listen address and serve requests

```rust
// Simplified daemon bootstrap (crates/oored/src/main.rs)
let store = SetupStore::connect(db_path).await?;
let initial = store.init_if_missing().await?;
let encryption_key = crypto::load_or_generate_key(&key_path)?;
let app = build_router(store, encryption_key);
axum::serve(listener, app).await?;
```

## Application state

All handlers share an `Arc<AppState>` containing:

| Field | Type | Purpose |
|-------|------|---------|
| `store` | `Mutex<SetupStore>` | SQLite-backed persistent state |
| `sessions` | `Mutex<SessionStore>` | In-memory authenticated sessions |
| `pending_auth` | `Mutex<HashMap<String, PendingAuth>>` | Pending OIDC authorization flows |
| `encryption_key` | `Vec<u8>` | AES-256 key for secrets at rest |

## Setup state machine

The daemon enforces a strict state machine for instance setup. Each setup endpoint is gated by the current state, and transitions are forward-only.

```
bootstrap_pending ──▶ idp_configured ──▶ owner_created ──▶ ready
```

| State | What it means | Next action |
|-------|---------------|-------------|
| `bootstrap_pending` | Fresh instance, awaiting bootstrap token | Verify token via `POST /v1/setup/bootstrap-token/verify` |
| `idp_configured` | OIDC provider has been configured | Create owner via `POST /v1/setup/owner/start-oidc` and `POST /v1/setup/owner/verify-oidc` |
| `owner_created` | Owner account exists, setup not finalized | Complete setup via `POST /v1/setup/complete` |
| `ready` | Instance is fully configured | All setup endpoints return `409 Conflict` |

::: danger
Once the state reaches `ready`, all setup mutating endpoints are permanently disabled. There is no way to re-enter setup mode through the API. This is a deliberate security design.
:::

## Data layer

### SQLite store

The `SetupStore` wraps a `SqlitePool` and provides `load()` and `save()` methods that serialize the entire `SetupStateFile` struct to a single row in the `setup_state` table.

```sql
CREATE TABLE setup_state (
    id                          INTEGER PRIMARY KEY CHECK (id = 1),
    schema_version              INTEGER NOT NULL,
    instance_id                 TEXT    NOT NULL,
    setup_state                 TEXT    NOT NULL,
    bootstrap_token_hash        TEXT,
    bootstrap_token_expires_at  INTEGER,
    bootstrap_token_consumed_at INTEGER,
    session_hash                TEXT,
    session_expires_at          INTEGER,
    oidc_issuer_url             TEXT,
    oidc_client_id              TEXT,
    -- ... additional OIDC and owner fields
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
);
```

The table is constrained to a single row (`CHECK (id = 1)`) since the daemon manages one instance per database.

### Encryption at rest

OIDC client secrets are encrypted with AES-256-GCM before storage:

1. A 256-bit key is generated on first run and stored at `~/Library/Application Support/oore/encryption.key` with `0o600` permissions
2. Each encryption operation generates a fresh random nonce
3. The ciphertext format is `base64(nonce || ciphertext || tag)`

## API routes

### Public endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/healthz` | Health check |
| `GET` | `/v1/public/setup-status` | Non-sensitive setup progress |

### Setup endpoints (session required)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/setup/bootstrap-token/verify` | Consume bootstrap token, get session |
| `POST` | `/v1/setup/oidc/configure` | Configure OIDC provider (with discovery) |
| `POST` | `/v1/setup/owner/start-oidc` | Start OIDC flow for owner verification |
| `POST` | `/v1/setup/owner/verify-oidc` | Complete OIDC flow, create owner |
| `POST` | `/v1/setup/complete` | Transition to `ready` state |

### Auth endpoints (ready state only)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/v1/auth/oidc/start` | Start OIDC login flow |
| `GET` | `/v1/auth/oidc/callback` | Handle OIDC callback, create session |
| `POST` | `/v1/auth/logout` | Revoke session |

## Session management

### Setup sessions

After bootstrap token verification, the daemon issues a setup session with a 30-minute sliding-window TTL. Each successful API call resets the TTL. The session hash is stored in the setup state database.

### Auth sessions

After setup is complete and a user authenticates via OIDC, the daemon creates an in-memory session with a 24-hour TTL. Session tokens are hashed with SHA-256 before storage; the plaintext is never persisted.

## OIDC integration

The daemon supports two OIDC flows:

### Setup OIDC flow

Used during setup to verify the owner's identity:

1. Frontend calls `POST /v1/setup/owner/start-oidc` with a `redirect_uri`
2. Daemon performs OIDC discovery, generates PKCE challenge, and returns an authorization URL
3. User authenticates with the identity provider
4. Callback returns `code` and `state` to the frontend
5. Frontend calls `POST /v1/setup/owner/verify-oidc` with the code
6. Daemon exchanges the code for tokens, validates the ID token, and creates the owner

### Production auth flow

Used after setup for regular authentication:

1. `GET /v1/auth/oidc/start` initiates the flow
2. `GET /v1/auth/oidc/callback` handles the redirect, validates the ID token, and creates a session

Both flows use PKCE (`S256`), CSRF state tokens, and nonce validation.

## CORS configuration

The daemon restricts cross-origin requests to approved frontend origins:

```rust
CorsLayer::new()
    .allow_origin(["http://localhost:3000".parse().unwrap()])
    .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
    .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
```

## Dependencies

Key Rust crates used by the daemon:

| Crate | Version | Purpose |
|-------|---------|---------|
| `axum` | 0.8 | HTTP framework |
| `tokio` | 1.48 | Async runtime |
| `sqlx` | 0.8 | SQLite database |
| `openidconnect` | 4 | OIDC discovery and token exchange |
| `ring` | 0.17 | AES-256-GCM encryption |
| `tower-http` | 0.6 | CORS middleware |
| `tracing` | 0.1 | Structured logging |
| `serde` / `serde_json` | 1.0 | Serialization |
