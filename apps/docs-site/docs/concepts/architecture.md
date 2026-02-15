---
status: implemented
description: "Understand oore.build system architecture including daemon, CLI, and web UI components."
---

# System Architecture

This page explains how oore.build's components fit together and why the system is designed this way.

## Components

oore.build consists of three components that communicate over HTTP:

```
┌─────────────┐     HTTPS/API     ┌──────────────┐
│   Web UI    │ ◄───────────────► │    oored     │
│  (React)    │                   │   (daemon)   │
└─────────────┘                   └──────┬───────┘
                                         │
                                         │ SQLite
                                         │
                                  ┌──────┴───────┐
                                  │   oore.db    │
                                  └──────────────┘

┌─────────────┐     HTTP/CLI      ┌──────────────┐
│    oore     │ ◄───────────────► │    oored     │
│   (CLI)     │                   │   (daemon)   │
└─────────────┘                   └──────────────┘
```

### oored (daemon)

The daemon is the central process. It:

- Serves the REST API (Axum web framework)
- Manages all persistent state in SQLite
- Handles OIDC authentication flows
- Schedules and tracks builds
- Runs an embedded build runner (default mode)
- Receives webhooks from GitHub and GitLab
- Manages artifact storage (local filesystem, S3, or R2)

The daemon runs on macOS only in V1 (required for iOS/macOS builds via Xcode).

### oore (CLI)

The operator CLI communicates with the daemon over HTTP. It handles:

- Instance setup (bootstrap token generation, interactive setup wizard)
- External runner registration and management
- Diagnostic checks (`oore doctor`)

The CLI shares the same SQLite database as the daemon for bootstrap token operations.

### Web UI

The React-based web UI connects to the daemon's API. It provides:

- Setup wizard (alternative to CLI setup)
- Project and pipeline management
- Build triggering and log streaming
- User invitation and role management
- Instance settings configuration
- Multi-instance support (connect to multiple backends)

The hosted UI at `ci.oore.build` is UI-only — it connects to the customer's self-hosted daemon. No build data passes through the hosted frontend.

## Data flow

### Build lifecycle

1. **Trigger** — User clicks "Build" in UI, pushes code (webhook), or calls the API
2. **Queue** — Daemon creates a build record in SQLite with status `queued`
3. **Claim** — Runner polls for available jobs and claims the build
4. **Execute** — Runner clones the repository, installs Flutter via FVM, runs build commands
5. **Stream** — Runner sends log lines to the daemon in real-time
6. **Complete** — Runner reports success/failure, uploads artifacts
7. **Store** — Daemon stores artifacts in configured storage (local, S3, or R2)
8. **Download** — Users download artifacts via signed, time-limited URLs

### Authentication flow

1. User clicks "Sign in" in the web UI
2. UI calls `GET /v1/auth/oidc/start` to get an authorization URL
3. User authenticates with their OIDC provider
4. Provider redirects back with an authorization code
5. UI sends the code to `POST /v1/auth/oidc/callback`
6. Daemon exchanges the code for tokens, verifies the ID token, creates a session
7. UI receives a session token (24-hour TTL)

## Why self-hosted?

Mobile CI requires macOS hardware for iOS and macOS builds (Xcode is macOS-only). Rather than renting Mac hardware from cloud providers, oore.build runs on your own Mac:

- **Signing keys stay on your hardware** — no need to upload certificates to a third party
- **Build artifacts are local** — distribute internally without external hosting
- **No per-minute billing** — your hardware, your schedule
- **Full control** — configure OIDC, RBAC, and storage as needed

## Why OIDC-only?

oore.build uses your existing identity provider (OIDC) for any non-loopback access (External Access / `runtime_mode=remote`):

- **No password storage** — eliminates an entire class of security concerns
- **Single sign-on** — users authenticate with the same credentials they use everywhere else
- **Centralized access control** — disable a user in your IdP and they lose access to oore.build
- **Enterprise ready** — works with Google Workspace, Okta, Azure AD, Auth0, Keycloak, and any OIDC-compliant provider

The daemon also supports loopback-only local login (no OIDC) for local-first onboarding and local operator access (auto-bootstrap requires Local Only mode).

## Technology choices

| Component | Technology | Why |
|---|---|---|
| Backend language | Rust | Performance, memory safety, single binary deployment |
| Web framework | Axum | Async, tower middleware ecosystem, type-safe extractors |
| Database | SQLite | Zero-config, single-file backup, sufficient for single-host |
| Frontend | React 19 + TanStack Router | File-based routing, type-safe, modern React patterns |
| Server state | TanStack Query | Cache management, background refetch, optimistic updates |
| Package manager | Bun | Fast installs, native bundler support |
| Flutter management | FVM | Per-project Flutter version pinning |
