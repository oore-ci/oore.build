# Backend Architecture

The oore.build backend consists of two Rust binaries (`oored` and `oore`) and one shared library crate (`oore-contract`). This page covers the daemon's internals.

## Daemon startup

When you run `oored run`, the daemon performs these initialization steps:

1. **Parse CLI args** -- listen address, optional state file path
2. **Initialize tracing** -- `tracing_subscriber` with `fmt` layer + optional OpenTelemetry layer (enabled when `OTEL_EXPORTER_OTLP_ENDPOINT` is set)
3. **Install Prometheus metrics** -- recorder for `http_requests_total` and `http_request_duration_seconds`
4. **Resolve database path** -- from `--state-file`, `OORE_SETUP_STATE_FILE` env, or default (`~/Library/Application Support/oore/oore.db`)
5. **Connect to SQLite** -- create the database file and run embedded migrations
6. **Initialize state** -- if no state row exists, create one with `BootstrapPending` and a fresh UUID instance ID
7. **Load encryption key** -- on macOS, use Keychain first (`build.oore.oored` / `encryption-key-v1`) with legacy file migration from `~/Library/Application Support/oore/encryption.key` when present
8. **Start embedded local runner (default mode)** -- bootstrap/rotate local runner credentials and start claim loop unless `OORED_RUNNER_MODE=external`
9. **Build Axum router** -- mount all route handlers, `/metrics` endpoint, and request metrics middleware
10. **Start HTTP server** -- bind to the listen address and serve requests

```rust
// Simplified daemon bootstrap (crates/oored/src/main.rs)
observability::init_tracing();
let metrics_handle = observability::init_metrics();
let store = SetupStore::connect(db_path).await?;
let initial = store.init_if_missing().await?;
let runtime_key = crypto::load_runtime_key()?;
let app = build_router(store, runtime_key.key, metrics_handle);
axum::serve(listener, app).await?;
observability::shutdown_tracing();
```

## Application state

All handlers share an `Arc<AppState>` containing:

| Field | Type | Purpose |
|-------|------|---------|
| `store` | `Mutex<SetupStore>` | SQLite-backed persistent state |
| `sessions` | `SessionStore` | Persistent auth sessions |
| `pending_auth` | `Mutex<HashMap<String, PendingAuth>>` | Pending OIDC authorization flows |
| `encryption_key` | `Vec<u8>` | AES-256 key for secrets at rest |
| `storage` | `RwLock<StorageBackend>` | Runtime artifact backend (`disabled`, `local`, `s3`, `r2`) |

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

1. On macOS, a 256-bit key is stored in Keychain (`build.oore.oored` / `encryption-key-v1`); legacy file key is migrated when present
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

## Runner modes

`oored` supports three runtime modes for build execution:

| Mode | Behavior |
|------|----------|
| `embedded` (default) | Start a local embedded runner process loop inside daemon runtime. |
| `external` | Disable embedded runner; requires separately started external runner(s). |
| `hybrid` | Start embedded runner and still allow external runners. |

Configure via `OORED_RUNNER_MODE`.

## Artifact storage backends

Artifact binary storage is runtime-configurable through Settings/API:

- `disabled`: metadata only, binary download links unavailable
- `local`: daemon-host filesystem with signed upload/download URLs
- `s3`: S3-compatible object storage via pre-signed URLs
- `r2`: Cloudflare R2 via S3-compatible endpoint

Configuration endpoint:

- `GET /v1/settings/artifact-storage`
- `PUT /v1/settings/artifact-storage`

Credentials for S3/R2 are encrypted at rest using daemon key material.

## Pipeline config resolution

Build snapshots now capture immutable config resolution metadata (`snapshot_version = 2`):

- `config_resolution_policy: file_first_ui_fallback`
- `config_path`
- `config_path_explicit`
- `ui_execution_config`

Runner resolution order:

1. Explicit mode: check explicit `config_path` only.
2. Auto mode: check `.oore.yaml`, then `.oore.yml`.
3. If file exists, parse strict YAML schema and execute it.
4. If file is missing, execute `ui_execution_config` fallback from snapshot.
5. If file exists but is invalid, fail build immediately.
6. Flutter version resolution:
   - `.fvmrc` in repository takes precedence
   - else use `ui_execution_config.flutter_version` when configured
   - when resolved, runner prepends `fvm use <version> --force` and runs Flutter/Dart via `fvm`

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
    .allow_origin(resolve_cors_origins()) // defaults: localhost:3000 + https://ci.oore.build
    .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE, Method::OPTIONS])
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
| `opentelemetry` | 0.28 | OpenTelemetry API (opt-in via `OTEL_EXPORTER_OTLP_ENDPOINT`) |
| `tracing-opentelemetry` | 0.29 | Bridge tracing spans to OpenTelemetry |
| `metrics` | 0.24 | Metrics facade (request counters and histograms) |
| `metrics-exporter-prometheus` | 0.16 | Prometheus text format exporter (`GET /metrics`) |
| `serde` / `serde_json` | 1.0 | Serialization |
