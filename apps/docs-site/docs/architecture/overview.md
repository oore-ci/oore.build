# Architecture Overview

oore.build is organized as a monorepo with clearly separated frontend and backend concerns. The frontend is a standalone React application that communicates with the backend over HTTPS APIs -- there is no server-side rendering or coupled deployment.

## Workspace layout

```
oore.build/
├── apps/
│   ├── web/              # Product web UI (React 19 + Vite)
│   └── docs-site/        # Documentation site (VitePress)
├── crates/
│   ├── oored/            # Daemon runtime and control plane (Axum)
│   ├── oore-runner/      # Shared runner execution engine
│   ├── oore/             # Operator CLI/TUI (Clap)
│   └── oore-contract/    # Shared data types (Serde structs)
├── docs/
│   ├── features/         # Required feature documentation entries
│   ├── platform-contract.md
│   └── strict-guidelines.md
├── scripts/              # Build and validation scripts
├── Cargo.toml            # Rust workspace root
├── Makefile              # Unified build targets
└── package.json          # Bun workspace root
```

## Technology stack

### Backend (Rust)

| Concern | Technology |
|---------|-----------|
| Language | Rust (edition 2024) |
| Async runtime | Tokio |
| HTTP framework | Axum 0.8 |
| CLI parsing | Clap 4.5 |
| Database | SQLite via sqlx |
| Authentication | OIDC via `openidconnect` |
| Encryption | AES-256-GCM via `ring` |
| Observability | `tracing`, OpenTelemetry, Prometheus metrics |

### Frontend (TypeScript)

| Concern | Technology |
|---------|-----------|
| Framework | React 19 |
| Build tool | Vite |
| Routing | TanStack Router (file-based) |
| Server state | TanStack Query |
| Client state | Zustand |
| UI components | shadcn with Base UI primitives |
| Styling | Tailwind CSS v4 |
| Icons | Hugeicons |
| Validation | Zod + React Hook Form |
| Package manager | Bun |

::: warning
Next.js is explicitly not used. TanStack Router with file-based routing is a non-negotiable V1 decision.
:::

## Runtime components

oore.build has three runtime components with distinct responsibilities:

### `oored` -- Daemon

The daemon is the central process that runs on the macOS host. It provides:

- HTTP API server (Axum) for the web UI and CLI
- Setup state machine management
- OIDC authentication and session management
- Encryption key management for secrets at rest
- Build scheduling and runner coordination
- Default embedded local runner for single-host execution
- Artifact and log API surface for build outputs

### `oore` -- Operator CLI

The CLI is the operator-facing tool for setup and administration:

- `oore setup open` -- generate a time-bound bootstrap token
- `oore setup` -- interactive setup wizard with OIDC loopback
- `oore runner register` -- register external runner hosts
- `oore runner start` -- run external runner process
- Future: `oore login`, `oore status`, `oore doctor`

### `oore-contract` -- Shared types

A pure library crate containing Serde structs shared between the daemon and CLI:

- `SetupState` enum and state machine transitions
- API request/response types
- `SetupStateFile` (the persistent state model)
- Error types (`ApiError`)

## API boundary

Frontend and backend are cleanly separated. All communication happens over versioned HTTP endpoints under `/v1`.

```
Frontend (React) ──── HTTPS ────▶ Backend (Axum)
                                  │
                                  ├── /v1/public/*     (read-only, no auth)
                                  ├── /v1/setup/*      (setup token required)
                                  ├── /v1/auth/*       (OIDC flows)
                                  ├── /v1/projects/*   (session required)
                                  └── /v1/builds/*     (session required)
```

The hosted offering at `ci.oore.build` serves only the frontend. The user's browser connects directly to their self-hosted `oored` instance.

## Security model

- **OIDC-only authentication** -- no local passwords in V1
- **One-time bootstrap tokens** -- TTL-bound, consumed on first use
- **Encrypted secrets at rest** -- AES-256-GCM for OIDC client secrets
- **Session tokens stored as hashes** -- plaintext never persisted
- **CORS restricted** -- only approved frontend origins
- **Setup auto-lockdown** -- mutating setup endpoints disabled after `ready`

## Extensibility

The architecture is designed to support future additions without breaking changes:

- Additional project types beyond Flutter
- Additional backend host platforms beyond macOS
- Desktop client (Tauri) after web-first release
- Multi-org/workspace tenancy (internal IDs avoid hard-coded assumptions)
- gRPC runner protocol (transport abstraction kept clean)

## Deep dives

- [Backend architecture](/architecture/backend) -- daemon internals, state machine, data layer
- [Frontend architecture](/architecture/frontend) -- routing, state management, multi-instance support
- [CLI architecture](/architecture/cli) -- setup flow, OIDC loopback, command contract
